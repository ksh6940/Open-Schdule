//! OpenSchedule CLI 모듈.
//! 외부 AI 에이전트(Claude Code, Antigravity, Cursor 등) 및 터미널 사용자가
//! 명령줄에서 일정을 추가/조회/수정/상태변경/삭제할 수 있도록 지원합니다.

use std::io::{self, Write};

use anyhow::{anyhow, bail, Result};
use chrono::{Duration, Local, NaiveDate, NaiveTime};
use serde_json::json;

use crate::{
    model::{Status, TodoDraft},
    DateFilter, Store, TodoQuery,
};

/// 실행 모드 판별 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAction {
    /// GUI 프로그램 실행 (`openlab start -s program` 또는 인자 없음)
    RunGui,
    /// 대화형 CLI 실행 (`openlab start -s`)
    RunInteractiveCli,
    /// 단일 CLI 명령 실행 (`openlab start -s <cmd>` 또는 `openschedule <cmd>`)
    RunCliCommand(Vec<String>),
}

/// 커맨드라인 인자를 분석하여 GUI 실행 / CLI 실행을 판별합니다.
pub fn parse_launch_action(raw_args: &[String]) -> LaunchAction {
    if raw_args.is_empty() {
        return LaunchAction::RunGui;
    }

    // `openlab start -s` 및 `openlab start -s program` 처리
    if raw_args[0].eq_ignore_ascii_case("start") {
        let rest = &raw_args[1..];
        if rest.is_empty() {
            return LaunchAction::RunInteractiveCli;
        }
        if rest[0] == "-s" || rest[0] == "--schedule" {
            let after_s = &rest[1..];
            if after_s.is_empty() {
                return LaunchAction::RunInteractiveCli;
            }
            if after_s.len() == 1
                && matches!(
                    after_s[0].to_lowercase().as_str(),
                    "program" | "gui" | "app" | "window"
                )
            {
                return LaunchAction::RunGui;
            }
            return LaunchAction::RunCliCommand(after_s.to_vec());
        }
        if rest.len() == 1
            && matches!(
                rest[0].to_lowercase().as_str(),
                "program" | "gui" | "app" | "window"
            )
        {
            return LaunchAction::RunGui;
        }
        return LaunchAction::RunCliCommand(rest.to_vec());
    }

    if raw_args.len() == 1
        && matches!(
            raw_args[0].to_lowercase().as_str(),
            "program" | "gui" | "--gui"
        )
    {
        return LaunchAction::RunGui;
    }

    if raw_args.len() == 1 && matches!(raw_args[0].to_lowercase().as_str(), "cli" | "--cli" | "-s")
    {
        return LaunchAction::RunInteractiveCli;
    }

    LaunchAction::RunCliCommand(raw_args.to_vec())
}

/// CLI 서브커맨드인지 판별합니다.
pub fn is_cli_invocation(args: &[String]) -> bool {
    !matches!(parse_launch_action(args), LaunchAction::RunGui)
}

/// `openlab start -s` 입력 시 실행되는 심플 CLI 모드 (이전 버전 디자인 유지)
pub fn run_interactive_cli() -> Result<()> {
    let store = Store::open_default()?;
    print_help();
    println!();

    let stdin = io::stdin();
    loop {
        print!("openschedule> ");
        io::stdout().flush().ok();

        let mut line = String::new();
        let bytes = stdin.read_line(&mut line)?;
        if bytes == 0 {
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if matches!(
            trimmed.to_lowercase().as_str(),
            "exit" | "quit" | "q" | "종료"
        ) {
            break;
        }

        let mut tokens = split_cli_tokens(trimmed);
        if tokens
            .first()
            .map(|s| s.eq_ignore_ascii_case("openschedule") || s.eq_ignore_ascii_case("openlab"))
            .unwrap_or(false)
        {
            tokens.remove(0);
        }
        if tokens.len() >= 2 && tokens[0].eq_ignore_ascii_case("start") && tokens[1] == "-s" {
            tokens.drain(0..2);
        }
        if tokens.is_empty() {
            continue;
        }

        if let Err(e) = execute_with_store(&store, &tokens) {
            eprintln!("오류: {e}");
        }
    }

    Ok(())
}

pub fn run_cli(args: &[String]) -> Result<()> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }

    if matches!(args[0].as_str(), "--version" | "-V") {
        println!("openschedule {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let store = Store::open_default()?;
    execute_with_store(&store, args)
}

fn execute_with_store(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }

    let cmd = args[0].as_str();
    let rest = &args[1..];

    match cmd {
        "add" | "create" | "new" | "추가" | "등록" => cmd_add(store, rest),
        "list" | "ls" | "목록" | "조회" => cmd_list(store, rest),
        "get" | "show" => cmd_get(store, rest),
        "done" | "complete" | "완료" => cmd_done(store, rest),
        "status" | "상태" => cmd_status(store, rest),
        "update" | "edit" | "수정" => cmd_update(store, rest),
        "delete" | "rm" | "remove" | "삭제" => cmd_delete(store, rest),
        "categories" | "cats" | "category" | "카테고리" => cmd_categories(store, rest),
        "summary" | "counts" | "요약" => cmd_summary(store, rest),
        "sync" | "load" | "동기화" => {
            let count = store.sync_all_json_files()?;
            println!("✓ 일자별 {{date}}_schedule.json 동기화 완료 ({count}개 파일)");
            Ok(())
        }
        other => {
            bail!("알 수 없는 명령어입니다: '{other}'\n사용법을 보려면 `openschedule --help`를 실행하세요.");
        }
    }
}

pub fn print_help() {
    println!(
        r##"OpenSchedule CLI - 일정 관리 명령줄 도구 (AI 에이전트 & 터미널 연동)

사용법:
  openlab start -s                     CLI 실행
  openlab start -s program             GUI 프로그램 실행
  openschedule <COMMAND> [OPTIONS]     단일 명령 실행

명령어 (Commands):
  add, 추가       새 일정 추가
                  예: openschedule add "팀 주간 회의" --date tomorrow --time 14:00 --category 업무 --memo "분기 목표 점검"
                  예: openschedule add "운동하기" -d 오늘 -t 19:00 -c 건강
  list, 조회      일정 목록 조회
                  예: openschedule list                          (전체 조회)
                  예: openschedule list --filter today           (오늘 일정)
                  예: openschedule list --filter scheduled       (예정 일정)
                  예: openschedule list --category 업무 --json   (JSON 포맷 출력)
  done, 완료      일정을 완료(또는 조건부완료) 처리
                  예: openschedule done 1
                  예: openschedule done 2 --condition "검토 대기"
  status, 상태    일정 상태 변경 (planned | in_progress | conditional_done | done)
                  예: openschedule status 1 in_progress
  update, 수정    일정 상세 정보 수정
                  예: openschedule update 1 --title "수정된 제목" --date 2026-11-15 --time 10:30
  delete, 삭제    일정 삭제
                  예: openschedule delete 1
  categories      카테고리(목록) 조회 / 추가 / 삭제
                  예: openschedule categories
                  예: openschedule categories add "프로젝트" --color "#AF52DE"
  summary, 요약   스마트 카운트 및 오늘의 일정 요약 출력
                  예: openschedule summary [--json]

공통 옵션:
  -d, --date <DATE>        날짜: YYYY-MM-DD, MM-DD, today(오늘), tomorrow(내일), 모레, next-week(다음주), +3d
  -t, --time <TIME>        시간: HH:MM (예: 09:00, 14:30), 오후 3시, 15시, 또는 none(삭제)
  -c, --category <NAME>    카테고리 이름 또는 ID (없는 이름 입력 시 자동 생성)
  -m, --memo <TEXT>        메모 내용
  -s, --status <STATUS>    상태: planned(진행예정), in_progress(진행중), conditional_done(조건부완료), done(완료)
      --condition <TEXT>   조건부완료 시 조건 메모
      --json               AI 및 스크립트 파싱을 위한 JSON 출력"##
    );
}

fn split_cli_tokens(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut quote_char = '"';

    for ch in input.chars() {
        if in_quotes {
            if ch == quote_char {
                in_quotes = false;
            } else {
                current.push(ch);
            }
        } else if ch == '"' || ch == '\'' {
            in_quotes = true;
            quote_char = ch;
        } else if ch.is_whitespace() {
            if !current.is_empty() {
                tokens.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

/// 유연한 날짜 문자열 파서
pub fn parse_flexible_date(input: &str) -> Result<NaiveDate> {
    let s = input.trim().to_lowercase();
    let today = Local::now().date_naive();

    match s.as_str() {
        "today" | "오늘" | "금일" | "now" => return Ok(today),
        "tomorrow" | "tmr" | "내일" | "명일" => return Ok(today + Duration::days(1)),
        "day-after-tomorrow" | "모레" | "내일모레" => return Ok(today + Duration::days(2)),
        "yesterday" | "어제" => return Ok(today - Duration::days(1)),
        "next-week" | "nextweek" | "다음주" | "다음 주" => return Ok(today + Duration::days(7)),
        _ => {}
    }

    // +Nd 또는 +Nw 형식 (예: +3d, +2w)
    if let Some(rest) = s.strip_prefix('+') {
        if let Some(days_str) = rest.strip_suffix('d').or_else(|| rest.strip_suffix("일")) {
            if let Ok(d) = days_str.trim().parse::<i64>() {
                return Ok(today + Duration::days(d));
            }
        }
        if let Some(weeks_str) = rest.strip_suffix('w').or_else(|| rest.strip_suffix("주")) {
            if let Ok(w) = weeks_str.trim().parse::<i64>() {
                return Ok(today + Duration::days(w * 7));
            }
        }
    }

    // YYYY-MM-DD 또는 YYYY/MM/DD 또는 YYYY.MM.DD
    let normalized = s.replace(['/', '.'], "-");
    if let Ok(d) = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
        return Ok(d);
    }

    // MM-DD 형식 (올해 기준)
    let parts: Vec<&str> = normalized.split('-').collect();
    if parts.len() == 2 {
        if let (Ok(m), Ok(d)) = (parts[0].parse::<u32>(), parts[1].parse::<u32>()) {
            use chrono::Datelike;
            if let Some(date) = NaiveDate::from_ymd_opt(today.year(), m, d) {
                return Ok(date);
            }
        }
    }

    bail!("날짜 형식을 인식할 수 없습니다: '{input}' (예: 2026-11-10, today, tomorrow, 오늘, 내일, +3d)")
}

/// 유연한 시간 문자열 파서
pub fn parse_flexible_time(input: &str) -> Result<Option<NaiveTime>> {
    let s = input.trim().to_lowercase();
    if s.is_empty() || matches!(s.as_str(), "none" | "null" | "clear" | "삭제" | "없음") {
        return Ok(None);
    }

    if let Ok(t) = NaiveTime::parse_from_str(&s, "%H:%M") {
        return Ok(Some(t));
    }
    if let Ok(t) = NaiveTime::parse_from_str(&s, "%H:%M:%S") {
        return Ok(Some(t));
    }

    // "오후 3시 30분", "오전 9시", "15시" 등 처리
    let is_pm = s.contains("오후") || s.contains("pm");
    let is_am = s.contains("오전") || s.contains("am");
    let cleaned = s
        .replace("오후", "")
        .replace("오전", "")
        .replace("pm", "")
        .replace("am", "")
        .replace('분', "")
        .trim()
        .to_string();

    if cleaned.contains('시') {
        let parts: Vec<&str> = cleaned.split('시').map(|p| p.trim()).collect();
        if let Ok(mut hour) = parts[0].parse::<u32>() {
            let min = if parts.len() > 1 && !parts[1].is_empty() {
                parts[1].parse::<u32>().unwrap_or(0)
            } else {
                0
            };
            if is_pm && hour < 12 {
                hour += 12;
            } else if is_am && hour == 12 {
                hour = 0;
            }
            if let Some(t) = NaiveTime::from_hms_opt(hour, min, 0) {
                return Ok(Some(t));
            }
        }
    }

    // 단순 숫자 시간 (예: "15" -> 15:00)
    if let Ok(mut hour) = cleaned.parse::<u32>() {
        if is_pm && hour < 12 {
            hour += 12;
        }
        if let Some(t) = NaiveTime::from_hms_opt(hour, 0, 0) {
            return Ok(Some(t));
        }
    }

    bail!("시간 형식을 인식할 수 없습니다: '{input}' (예: 09:00, 14:30, 오후 3시)")
}

fn resolve_category(store: &Store, input: &str, create_if_missing: bool) -> Result<Option<i64>> {
    let trimmed = input.trim();
    if trimmed.is_empty()
        || matches!(
            trimmed.to_lowercase().as_str(),
            "none" | "null" | "미분류" | "없음"
        )
    {
        return Ok(None);
    }
    if let Ok(id) = trimmed.parse::<i64>() {
        let cats = store.categories()?;
        if cats.iter().any(|c| c.id == id) {
            return Ok(Some(id));
        }
    }
    store.find_category(trimmed, create_if_missing)
}

fn cmd_add(store: &Store, args: &[String]) -> Result<()> {
    let mut title_parts: Vec<String> = Vec::new();
    let mut due_date = Local::now().date_naive();
    let mut due_time: Option<NaiveTime> = None;
    let mut category_arg: Option<String> = None;
    let mut memo = String::new();
    let mut status = Status::Planned;
    let mut condition_note = String::new();
    let mut json_output = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--title" | "-T" => {
                i += 1;
                if i >= args.len() {
                    bail!("--title 옵션에 값이 필요합니다.");
                }
                title_parts.push(args[i].clone());
            }
            "--date" | "-d" | "--due" => {
                i += 1;
                if i >= args.len() {
                    bail!("--date 옵션에 날짜 값이 필요합니다.");
                }
                due_date = parse_flexible_date(&args[i])?;
            }
            "--time" | "-t" => {
                i += 1;
                if i >= args.len() {
                    bail!("--time 옵션에 시간 값이 필요합니다.");
                }
                due_time = parse_flexible_time(&args[i])?;
            }
            "--category" | "-c" | "--list" | "-l" => {
                i += 1;
                if i >= args.len() {
                    bail!("--category 옵션에 카테고리 이름 또는 ID가 필요합니다.");
                }
                category_arg = Some(args[i].clone());
            }
            "--memo" | "-m" | "--note" | "-n" => {
                i += 1;
                if i >= args.len() {
                    bail!("--memo 옵션에 내용이 필요합니다.");
                }
                memo = args[i].clone();
            }
            "--status" | "-s" => {
                i += 1;
                if i >= args.len() {
                    bail!("--status 옵션에 상태 값이 필요합니다.");
                }
                status = Status::parse_loose(&args[i])
                    .ok_or_else(|| anyhow!("알 수 없는 상태 값입니다: '{}'", args[i]))?;
            }
            "--condition" => {
                i += 1;
                if i >= args.len() {
                    bail!("--condition 옵션에 조건 메모가 필요합니다.");
                }
                condition_note = args[i].clone();
            }
            "--json" => {
                json_output = true;
            }
            arg if arg.starts_with('-') => {
                bail!("알 수 없는 옵션입니다: '{arg}'");
            }
            positional => {
                title_parts.push(positional.to_string());
            }
        }
        i += 1;
    }

    let title = title_parts.join(" ").trim().to_string();
    if title.is_empty() {
        bail!("일정 제목을 입력해주세요.\n예: openschedule add \"팀 회의\" --date tomorrow --time 14:00 --category 업무");
    }

    let category_id = match category_arg {
        Some(ref c) => resolve_category(store, c, true)?,
        None => None,
    };

    if !condition_note.is_empty() && status == Status::Planned {
        status = Status::ConditionalDone;
    }

    let draft = TodoDraft {
        title,
        memo,
        category_id,
        due_date,
        due_time,
        status,
        condition_note,
    };

    let id = store.add(&draft)?;
    let todo = store
        .get(id)?
        .ok_or_else(|| anyhow!("생성된 일정을 불러오지 못했습니다."))?;

    let cats = store.categories()?;
    let cat_name = todo
        .category_id
        .and_then(|cid| cats.iter().find(|c| c.id == cid).map(|c| c.name.as_str()))
        .unwrap_or("미분류");

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "added",
                "todo": todo,
                "category_name": cat_name,
            }))?
        );
    } else {
        let time_str = todo
            .due_time
            .map(|t| format!(" {}", t.format("%H:%M")))
            .unwrap_or_default();
        println!(
            "✓ 일정 등록 완료 [ID: #{}] \"{}\" | 기한: {}{} | 목록: {} | 상태: {}",
            todo.id,
            todo.title,
            todo.due_date.format("%Y-%m-%d"),
            time_str,
            cat_name,
            todo.status.label()
        );
    }

    Ok(())
}

fn cmd_list(store: &Store, args: &[String]) -> Result<()> {
    let mut filter = DateFilter::All;
    let mut category: Option<Option<i64>> = None;
    let mut statuses: Vec<Status> = Vec::new();
    let mut search = String::new();
    let mut json_output = false;

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--filter" | "-f" => {
                i += 1;
                if i >= args.len() {
                    bail!("--filter 옵션에 값이 필요합니다.");
                }
                filter = match args[i].to_lowercase().as_str() {
                    "today" | "오늘" => DateFilter::Today,
                    "today_and_overdue" | "overdue" | "오늘+지연" => DateFilter::TodayAndOverdue,
                    "scheduled" | "예정" | "upcoming" => DateFilter::Scheduled,
                    "week" | "this_week" | "이번주" => DateFilter::ThisWeek,
                    "completed" | "done" | "완료" | "완료됨" => DateFilter::Completed,
                    "all" | "전체" => DateFilter::All,
                    other => DateFilter::Day(parse_flexible_date(other)?),
                };
            }
            "--date" | "-d" => {
                i += 1;
                if i >= args.len() {
                    bail!("--date 옵션에 날짜 값이 필요합니다.");
                }
                filter = DateFilter::Day(parse_flexible_date(&args[i])?);
            }
            "--category" | "-c" | "--list" | "-l" => {
                i += 1;
                if i >= args.len() {
                    bail!("--category 옵션에 값이 필요합니다.");
                }
                let cid = resolve_category(store, &args[i], false)?;
                category = Some(cid);
            }
            "--status" | "-s" => {
                i += 1;
                if i >= args.len() {
                    bail!("--status 옵션에 상태 값이 필요합니다.");
                }
                let st = Status::parse_loose(&args[i])
                    .ok_or_else(|| anyhow!("알 수 없는 상태 값입니다: '{}'", args[i]))?;
                statuses.push(st);
            }
            "--search" | "-q" => {
                i += 1;
                if i >= args.len() {
                    bail!("--search 옵션에 검색어가 필요합니다.");
                }
                search = args[i].clone();
            }
            "--json" => {
                json_output = true;
            }
            positional if !positional.starts_with('-') => {
                filter = match positional.to_lowercase().as_str() {
                    "today" | "오늘" => DateFilter::TodayAndOverdue,
                    "scheduled" | "예정" => DateFilter::Scheduled,
                    "completed" | "완료" | "완료됨" => DateFilter::Completed,
                    "week" | "이번주" => DateFilter::ThisWeek,
                    "all" | "전체" => DateFilter::All,
                    other => DateFilter::Day(parse_flexible_date(other)?),
                };
            }
            other => bail!("알 수 없는 옵션입니다: '{other}'"),
        }
        i += 1;
    }

    let todos = store.query(&TodoQuery {
        date: filter,
        category,
        statuses,
        search,
    })?;
    let cats = store.categories()?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&todos)?);
        return Ok(());
    }

    if todos.is_empty() {
        println!("조회된 일정이 없습니다.");
        return Ok(());
    }

    println!("총 {}개의 일정:", todos.len());
    for t in &todos {
        let cat_name = t
            .category_id
            .and_then(|cid| cats.iter().find(|c| c.id == cid).map(|c| c.name.as_str()))
            .unwrap_or("미분류");
        let time_str = t
            .due_time
            .map(|tm| format!(" {}", tm.format("%H:%M")))
            .unwrap_or_default();
        let memo_str = if t.memo.trim().is_empty() {
            String::new()
        } else {
            format!(" — {}", t.memo.trim())
        };
        let cond_str = if t.condition_note.trim().is_empty() {
            String::new()
        } else {
            format!(" [조건: {}]", t.condition_note.trim())
        };
        println!(
            "  [#{:<3}] {} {:<6} | {}{} | [{}] {}{}{}",
            t.id,
            t.status.icon(),
            t.status.label(),
            t.due_date.format("%Y-%m-%d"),
            time_str,
            cat_name,
            t.title,
            cond_str,
            memo_str
        );
    }

    Ok(())
}

fn cmd_get(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("조회할 일정 ID를 입력해주세요. 예: openschedule get 1");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;
    let json_output = args.iter().any(|a| a == "--json");

    let todo = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;

    if json_output {
        println!("{}", serde_json::to_string_pretty(&todo)?);
    } else {
        println!("{}", serde_json::to_string_pretty(&todo)?);
    }
    Ok(())
}

fn cmd_done(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("완료 처리할 일정 ID를 입력해주세요. 예: openschedule done 1");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;

    let mut condition_note: Option<String> = None;
    let mut json_output = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--condition" | "-c" => {
                i += 1;
                if i >= args.len() {
                    bail!("--condition 옵션에 조건 메모가 필요합니다.");
                }
                condition_note = Some(args[i].clone());
            }
            "--json" => json_output = true,
            other => bail!("알 수 없는 옵션입니다: '{other}'"),
        }
        i += 1;
    }

    let existing = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;

    let mut draft = existing.to_draft();
    if let Some(cond) = condition_note {
        draft.status = Status::ConditionalDone;
        draft.condition_note = cond;
    } else {
        draft.status = Status::Done;
    }

    store.update(id, &draft)?;
    let updated = store.get(id)?.unwrap();

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "completed",
                "todo": updated,
            }))?
        );
    } else {
        println!(
            "✓ 일정 완료 처리됨 [ID: #{}] \"{}\" -> {}",
            updated.id,
            updated.title,
            updated.status.label()
        );
    }
    Ok(())
}

fn cmd_status(store: &Store, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        bail!("사용법: openschedule status <ID> <STATUS> [--condition <메모>]\n상태값: planned(진행예정), in_progress(진행중), conditional_done(조건부완료), done(완료)");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;
    let next_status = Status::parse_loose(&args[1])
        .ok_or_else(|| anyhow!("알 수 없는 상태 값입니다: '{}'", args[1]))?;

    let mut condition_note: Option<String> = None;
    let mut json_output = false;

    let mut i = 2;
    while i < args.len() {
        match args[i].as_str() {
            "--condition" | "-c" => {
                i += 1;
                if i >= args.len() {
                    bail!("--condition 옵션에 조건 메모가 필요합니다.");
                }
                condition_note = Some(args[i].clone());
            }
            "--json" => json_output = true,
            other => bail!("알 수 없는 옵션입니다: '{other}'"),
        }
        i += 1;
    }

    let existing = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;
    let mut draft = existing.to_draft();
    draft.status = next_status;
    if let Some(cond) = condition_note {
        draft.condition_note = cond;
    }

    store.update(id, &draft)?;
    let updated = store.get(id)?.unwrap();

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "status_updated",
                "todo": updated,
            }))?
        );
    } else {
        println!(
            "✓ 상태 변경됨 [ID: #{}] \"{}\" -> {}",
            updated.id,
            updated.title,
            updated.status.label()
        );
    }
    Ok(())
}

fn cmd_update(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("수정할 일정 ID를 입력해주세요. 예: openschedule update 1 --title \"새 제목\" --date tomorrow");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;

    let existing = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;
    let mut draft = existing.to_draft();
    let mut json_output = false;

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--title" | "-T" => {
                i += 1;
                if i >= args.len() {
                    bail!("--title 옵션에 값이 필요합니다.");
                }
                draft.title = args[i].clone();
            }
            "--date" | "-d" | "--due" => {
                i += 1;
                if i >= args.len() {
                    bail!("--date 옵션에 값이 필요합니다.");
                }
                draft.due_date = parse_flexible_date(&args[i])?;
            }
            "--time" | "-t" => {
                i += 1;
                if i >= args.len() {
                    bail!("--time 옵션에 값이 필요합니다.");
                }
                draft.due_time = parse_flexible_time(&args[i])?;
            }
            "--category" | "-c" | "--list" | "-l" => {
                i += 1;
                if i >= args.len() {
                    bail!("--category 옵션에 값이 필요합니다.");
                }
                draft.category_id = resolve_category(store, &args[i], true)?;
            }
            "--memo" | "-m" | "--note" | "-n" => {
                i += 1;
                if i >= args.len() {
                    bail!("--memo 옵션에 값이 필요합니다.");
                }
                draft.memo = args[i].clone();
            }
            "--status" | "-s" => {
                i += 1;
                if i >= args.len() {
                    bail!("--status 옵션에 값이 필요합니다.");
                }
                draft.status = Status::parse_loose(&args[i])
                    .ok_or_else(|| anyhow!("알 수 없는 상태 값입니다: '{}'", args[i]))?;
            }
            "--condition" => {
                i += 1;
                if i >= args.len() {
                    bail!("--condition 옵션에 값이 필요합니다.");
                }
                draft.condition_note = args[i].clone();
            }
            "--json" => json_output = true,
            other => bail!("알 수 없는 옵션입니다: '{other}'"),
        }
        i += 1;
    }

    store.update(id, &draft)?;
    let updated = store.get(id)?.unwrap();

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "updated",
                "todo": updated,
            }))?
        );
    } else {
        println!(
            "✓ 일정 수정 완료 [ID: #{}] \"{}\" (기한: {})",
            updated.id,
            updated.title,
            updated.due_date.format("%Y-%m-%d")
        );
    }
    Ok(())
}

fn cmd_delete(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("삭제할 일정 ID를 입력해주세요. 예: openschedule delete 1");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;
    let json_output = args.iter().any(|a| a == "--json");

    let existing = store.get(id)?;
    store.delete(id)?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "deleted",
                "id": id,
                "deleted_todo": existing,
            }))?
        );
    } else if let Some(t) = existing {
        println!("✓ 일정 삭제 완료 [ID: #{}] \"{}\"", id, t.title);
    } else {
        println!("✓ 일정 삭제 완료 [ID: #{}]", id);
    }
    Ok(())
}

fn cmd_categories(store: &Store, args: &[String]) -> Result<()> {
    let json_output = args.iter().any(|a| a == "--json");

    if args.is_empty() || (args.len() == 1 && json_output) {
        let cats = store.categories()?;
        let counts = store.category_counts()?;
        if json_output {
            println!("{}", serde_json::to_string_pretty(&cats)?);
        } else {
            println!("카테고리 목록 ({}개):", cats.len());
            for c in cats {
                let cnt = counts.get(&c.id).copied().unwrap_or(0);
                println!(
                    "  [#{:<2}] {} (#{:06X}) - 미완료 {}개",
                    c.id, c.name, c.color, cnt
                );
            }
        }
        return Ok(());
    }

    match args[0].as_str() {
        "add" | "create" | "추가" => {
            if args.len() < 2 {
                bail!("추가할 카테고리 이름을 입력해주세요. 예: openschedule categories add \"프로젝트\"");
            }
            let name = &args[1];
            let mut color: u32 = 0x0071E3;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--color" | "-c" => {
                        i += 1;
                        if i < args.len() {
                            let hex_str = args[i].trim_start_matches('#').trim_start_matches("0x");
                            color = u32::from_str_radix(hex_str, 16).unwrap_or(0x0071E3);
                        }
                    }
                    "--json" => {}
                    _ => {}
                }
                i += 1;
            }
            let id = store.add_category(name, color)?;
            if json_output {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({
                        "ok": true,
                        "id": id,
                        "name": name,
                        "color": color,
                    }))?
                );
            } else {
                println!("✓ 카테고리 추가됨 [ID: #{}] \"{}\" (#{:06X})", id, name, color);
            }
        }
        "delete" | "rm" | "삭제" => {
            if args.len() < 2 {
                bail!("삭제할 카테고리 ID를 입력해주세요.");
            }
            let id: i64 = args[1].trim_start_matches('#').parse()?;
            store.delete_category(id)?;
            println!("✓ 카테고리 삭제 완료 [ID: #{}]", id);
        }
        _ => bail!("알 수 없는 categories 하위 명령입니다: '{}'", args[0]),
    }

    Ok(())
}

fn cmd_summary(store: &Store, args: &[String]) -> Result<()> {
    let json_output = args.iter().any(|a| a == "--json");
    let counts = store.smart_counts()?;
    let today_todos = store.query(&TodoQuery {
        date: DateFilter::TodayAndOverdue,
        ..Default::default()
    })?;

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "counts": counts,
                "today_and_overdue": today_todos,
            }))?
        );
    } else {
        println!("📊 OpenSchedule 요약");
        println!(
            "  오늘(지연 포함): {}개 | 예정: {}개 | 전체 미완료: {}개 | 완료됨: {}개",
            counts.today, counts.scheduled, counts.all, counts.completed
        );
        println!("\n📌 오늘 및 지연된 일정:");
        print!("{}", store.context_text(DateFilter::TodayAndOverdue, 50)?);
    }
    Ok(())
}
