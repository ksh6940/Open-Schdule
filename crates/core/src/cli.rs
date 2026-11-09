//! OpenSchedule / OpenLab CLI 모듈.
//! - `openlab start -s` : 대화형 CLI 모드 실행 (또는 뒤에 하위 명령 전달 시 즉시 실행)
//! - `openlab start -s program` : GUI 데스크톱 프로그램 실행
//! - 일자별 `{date}_schedule.json` 로컬 저장 및 SQLite(`rusqlite`) 연동 관리

use std::io::{self, Write};

use anyhow::{anyhow, bail, Result};
use chrono::{Duration, Local, NaiveDate, NaiveTime};
use serde_json::json;

use crate::{
    model::{Status, TodoDraft},
    paths, DateFilter, Store, TodoQuery,
};

/// 실행 모드 판별 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAction {
    /// GUI 프로그램 실행 (`openlab start -s program` 또는 인자 없음)
    RunGui,
    /// 대화형 CLI 셸 실행 (`openlab start -s`)
    RunInteractiveCli,
    /// 단일 CLI 명령 실행 (`openlab start -s add ...` 또는 `openlab add ...`)
    RunCliCommand(Vec<String>),
}

/// 커맨드라인 인자를 분석하여 GUI 실행 / 대화형 CLI 실행 / 단일 CLI 명령 실행을 판별합니다.
pub fn parse_launch_action(raw_args: &[String]) -> LaunchAction {
    if raw_args.is_empty() {
        return LaunchAction::RunGui;
    }

    // `openlab start -s ...` 패턴 처리
    if raw_args[0].eq_ignore_ascii_case("start") {
        let rest = &raw_args[1..];
        if rest.is_empty() {
            return LaunchAction::RunInteractiveCli;
        }
        if rest[0] == "-s" || rest[0] == "--schedule" {
            let after_s = &rest[1..];
            if after_s.is_empty() {
                // `openlab start -s` -> CLI 실행!
                return LaunchAction::RunInteractiveCli;
            }
            if after_s.len() == 1
                && matches!(
                    after_s[0].to_lowercase().as_str(),
                    "program" | "gui" | "app" | "window"
                )
            {
                // `openlab start -s program` -> GUI 프로그램 실행!
                return LaunchAction::RunGui;
            }
            // `openlab start -s add ...` 등 하위 명령 실행
            return LaunchAction::RunCliCommand(after_s.to_vec());
        }
        if rest.len() == 1
            && matches!(
                rest[0].to_lowercase().as_str(),
                "program" | "gui" | "app"
            )
        {
            return LaunchAction::RunGui;
        }
        return LaunchAction::RunCliCommand(rest.to_vec());
    }

    // `openlab -s` 또는 `openlab -s program`
    if raw_args[0] == "-s" {
        let after_s = &raw_args[1..];
        if after_s.is_empty() {
            return LaunchAction::RunInteractiveCli;
        }
        if after_s.len() == 1 && after_s[0].eq_ignore_ascii_case("program") {
            return LaunchAction::RunGui;
        }
        return LaunchAction::RunCliCommand(after_s.to_vec());
    }

    if raw_args.len() == 1
        && matches!(
            raw_args[0].to_lowercase().as_str(),
            "cli" | "interactive" | "shell"
        )
    {
        return LaunchAction::RunInteractiveCli;
    }

    if raw_args.len() == 1
        && matches!(
            raw_args[0].to_lowercase().as_str(),
            "program" | "gui" | "app"
        )
    {
        return LaunchAction::RunGui;
    }

    if is_cli_invocation(raw_args) {
        return LaunchAction::RunCliCommand(raw_args.to_vec());
    }

    LaunchAction::RunCliCommand(raw_args.to_vec())
}

/// CLI 서브커맨드인지 판별합니다.
pub fn is_cli_invocation(args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }
    let first = args[0].as_str();
    matches!(
        first,
        "start"
            | "-s"
            | "cli"
            | "add"
            | "create"
            | "new"
            | "추가"
            | "등록"
            | "list"
            | "ls"
            | "목록"
            | "조회"
            | "get"
            | "show"
            | "done"
            | "complete"
            | "완료"
            | "status"
            | "상태"
            | "update"
            | "edit"
            | "수정"
            | "delete"
            | "rm"
            | "remove"
            | "삭제"
            | "categories"
            | "cats"
            | "category"
            | "카테고리"
            | "json"
            | "sync"
            | "load"
            | "동기화"
            | "summary"
            | "counts"
            | "요약"
            | "help"
            | "--help"
            | "-h"
            | "--version"
            | "-V"
    )
}

/// 대화형 CLI 모드 (`openlab start -s`) 실행
pub fn run_interactive_cli() -> Result<()> {
    let store = Store::open_default()?;
    let _ = store.sync_all_json_files();

    println!("======================================================================");
    println!("  🚀 OpenLab Schedule CLI (SQLite + {{date}}_schedule.json 연동)");
    println!("  📂 JSON 저장 경로: {}", paths::schedules_dir().display());
    println!("  🗄️ SQLite DB 경로: {}", paths::db_path().display());
    println!("======================================================================");
    println!("  [명령어 또는 번호를 입력하세요]");
    println!("   1 (또는 add)      : 새 할 일 추가 ({{date}}_schedule.json 자동 저장)");
    println!("   2 (또는 date)     : 일자별 할 일 조회 및 JSON 확인");
    println!("   3 (또는 list)     : 전체 / 오늘 / 예정 할 일 목록 조회");
    println!("   4 (또는 edit)     : 할 일 수정 (제목, 날짜, 시간, 카테고리, 메모)");
    println!("   5 (또는 status)   : 할 일 상태 변경 / 완료 처리");
    println!("   6 (또는 delete)   : 할 일 삭제");
    println!("   7 (또는 json)     : 저장된 {{date}}_schedule.json 파일 목록 & 동기화");
    println!("   help              : 전체 CLI 명령어 도움말 보기");
    println!("   0 (또는 exit, q)  : CLI 종료");
    println!("----------------------------------------------------------------------");
    println!("  💡 팁: 프롬프트에서 바로 `add \"팀 회의\" -d 내일 -t 14:00 -c 업무` 처럼");
    println!("         일반 CLI 명령어를 직접 입력하셔도 됩니다.");
    println!("======================================================================");

    let stdin = io::stdin();
    loop {
        print!("\nopenlab> ");
        io::stdout().flush()?;

        let mut line = String::new();
        if stdin.read_line(&mut line)? == 0 {
            break;
        }
        let input = line.trim();
        if input.is_empty() {
            continue;
        }

        match input.to_lowercase().as_str() {
            "0" | "exit" | "quit" | "q" | "종료" => {
                println!("OpenLab CLI를 종료합니다.");
                break;
            }
            "1" => {
                if let Err(e) = interactive_add(&store) {
                    eprintln!("오류: {e}");
                }
            }
            "2" | "date" => {
                if let Err(e) = interactive_date_view(&store) {
                    eprintln!("오류: {e}");
                }
            }
            "3" => {
                let _ = cmd_list(&store, &[]);
            }
            "4" => {
                if let Err(e) = interactive_edit(&store) {
                    eprintln!("오류: {e}");
                }
            }
            "5" => {
                if let Err(e) = interactive_status(&store) {
                    eprintln!("오류: {e}");
                }
            }
            "6" => {
                if let Err(e) = interactive_delete(&store) {
                    eprintln!("오류: {e}");
                }
            }
            "7" => {
                let _ = cmd_json(&store, &[]);
            }
            _ => {
                let tokens = split_shell_words(input);
                if !tokens.is_empty() {
                    if let Err(e) = run_cli_with_store(&store, &tokens) {
                        eprintln!("오류: {e}");
                    }
                }
            }
        }
    }

    Ok(())
}

fn prompt_line(label: &str) -> Result<String> {
    print!("{label}");
    io::stdout().flush()?;
    let mut buf = String::new();
    io::stdin().read_line(&mut buf)?;
    Ok(buf.trim().to_string())
}

fn interactive_add(store: &Store) -> Result<()> {
    let today = Local::now().date_naive();
    let title = prompt_line("  • 할 일 제목: ")?;
    if title.is_empty() {
        bail!("제목은 비워둘 수 없습니다.");
    }
    let date_in = prompt_line(&format!(
        "  • 날짜 (기본값: {} / 오늘, 내일, YYYY-MM-DD): ",
        today.format("%Y-%m-%d")
    ))?;
    let due_date = if date_in.is_empty() {
        today
    } else {
        parse_flexible_date(&date_in)?
    };

    let time_in = prompt_line("  • 시간 (선택, 예: 14:00, 오후 3시, Enter 생략): ")?;
    let due_time = if time_in.is_empty() {
        None
    } else {
        parse_flexible_time(&time_in)?
    };

    let cat_in = prompt_line("  • 카테고리 (선택, 예: 업무, 개인, 공부, 건강, Enter 미분류): ")?;
    let category_id = resolve_category(store, &cat_in, true)?;

    let memo = prompt_line("  • 메모 (선택, Enter 생략): ")?;

    let draft = TodoDraft {
        title,
        memo,
        category_id,
        due_date,
        due_time,
        status: Status::Planned,
        condition_note: String::new(),
    };

    let id = store.add(&draft)?;
    let json_path = paths::date_schedule_json_path(due_date);
    println!(
        "✓ 할 일 등록 완료 [ID: #{}] \"{}\" (날짜: {})",
        id,
        draft.title,
        due_date.format("%Y-%m-%d")
    );
    println!("  📄 JSON 저장됨: {}", json_path.display());
    Ok(())
}

fn interactive_date_view(store: &Store) -> Result<()> {
    let today = Local::now().date_naive();
    let date_in = prompt_line(&format!(
        "  • 조회할 날짜 입력 (기본값: {} / 오늘, 내일, YYYY-MM-DD): ",
        today.format("%Y-%m-%d")
    ))?;
    let date = if date_in.is_empty() {
        today
    } else {
        parse_flexible_date(&date_in)?
    };

    let json_path = paths::date_schedule_json_path(date);
    if json_path.exists() {
        let _ = store.import_date_json_file(&json_path);
    }
    println!(
        "\n📅 [{}] 할 일 목록 (파일: {}_schedule.json):",
        date.format("%Y-%m-%d"),
        date.format("%Y-%m-%d")
    );
    cmd_list(store, &["--date".to_string(), date.format("%Y-%m-%d").to_string()])
}

fn interactive_edit(store: &Store) -> Result<()> {
    cmd_list(store, &[])?;
    let id_str = prompt_line("\n  • 수정할 할 일 ID 번호: ")?;
    let id: i64 = id_str.trim_start_matches('#').parse()?;
    let existing = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;

    let mut draft = existing.to_draft();
    println!("  (변경하지 않으려면 Enter를 누르세요)");

    let new_title = prompt_line(&format!("  • 새 제목 [현재: {}]: ", draft.title))?;
    if !new_title.is_empty() {
        draft.title = new_title;
    }

    let new_date = prompt_line(&format!(
        "  • 새 날짜 [현재: {}]: ",
        draft.due_date.format("%Y-%m-%d")
    ))?;
    if !new_date.is_empty() {
        draft.due_date = parse_flexible_date(&new_date)?;
    }

    let cur_time = draft
        .due_time
        .map(|t| t.format("%H:%M").to_string())
        .unwrap_or_else(|| "없음".into());
    let new_time = prompt_line(&format!("  • 새 시간 [현재: {cur_time}, 삭제는 none]: "))?;
    if !new_time.is_empty() {
        draft.due_time = parse_flexible_time(&new_time)?;
    }

    let new_memo = prompt_line(&format!("  • 새 메모 [현재: {}]: ", draft.memo))?;
    if !new_memo.is_empty() {
        draft.memo = new_memo;
    }

    store.update(id, &draft)?;
    let json_path = paths::date_schedule_json_path(draft.due_date);
    println!("✓ 일정 수정 완료 [ID: #{id}]");
    println!("  📄 JSON 갱신됨: {}", json_path.display());
    Ok(())
}

fn interactive_status(store: &Store) -> Result<()> {
    cmd_list(store, &[])?;
    let id_str = prompt_line("\n  • 상태를 변경할 할 일 ID 번호: ")?;
    let id: i64 = id_str.trim_start_matches('#').parse()?;
    let st_str = prompt_line(
        "  • 변경할 상태 (1: 진행예정, 2: 진행중, 3: 완료, 4: 조건부완료) [기본 3 완료]: ",
    )?;
    let status = match st_str.as_str() {
        "1" => Status::Planned,
        "2" => Status::InProgress,
        "" | "3" => Status::Done,
        "4" => Status::ConditionalDone,
        other => Status::parse_loose(other)
            .ok_or_else(|| anyhow!("알 수 없는 상태입니다: {other}"))?,
    };
    let mut cond = String::new();
    if status == Status::ConditionalDone {
        cond = prompt_line("  • 조건부완료 사유: ")?;
    }

    let existing = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;
    let mut draft = existing.to_draft();
    draft.status = status;
    if !cond.is_empty() {
        draft.condition_note = cond;
    }
    store.update(id, &draft)?;
    println!(
        "✓ 상태 변경 완료 [ID: #{id}] -> {}",
        status.label()
    );
    Ok(())
}

fn interactive_delete(store: &Store) -> Result<()> {
    cmd_list(store, &[])?;
    let id_str = prompt_line("\n  • 삭제할 할 일 ID 번호: ")?;
    let id: i64 = id_str.trim_start_matches('#').parse()?;
    cmd_delete(store, &[id.to_string()])
}

fn split_shell_words(input: &str) -> Vec<String> {
    let mut words = Vec::new();
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
                words.push(std::mem::take(&mut current));
            }
        } else {
            current.push(ch);
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

pub fn run_cli(args: &[String]) -> Result<()> {
    let store = Store::open_default()?;
    let _ = store.sync_all_json_files();
    run_cli_with_store(&store, args)
}

pub fn run_cli_with_store(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h") {
        print_help();
        return Ok(());
    }

    if matches!(args[0].as_str(), "--version" | "-V") {
        println!("openlab / openschedule {}", env!("CARGO_PKG_VERSION"));
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
        "json" | "sync" | "load" | "동기화" => cmd_json(store, rest),
        "summary" | "counts" | "요약" => cmd_summary(store, rest),
        other => {
            bail!("알 수 없는 명령어입니다: '{other}'\n사용법을 보려면 `openlab start -s help`를 실행하세요.");
        }
    }
}

pub fn print_help() {
    println!(
        r##"OpenLab / OpenSchedule CLI - 일정 관리 명령줄 도구 (SQLite + {{date}}_schedule.json)

실행 방법:
  openlab start -s                     대화형 CLI 일정 관리 프로그램 실행
  openlab start -s program             GUI 데스크톱 프로그램(캘린더 뷰 / 투두리스트) 실행
  openlab start -s <COMMAND> [OPTIONS] 단일 CLI 명령 즉시 실행 (AI 에이전트 연동)

명령어 (Commands):
  add, 추가       새 일정 추가 (자동으로 schedules/{{date}}_schedule.json 생성/갱신)
                  예: openlab start -s add "팀 주간 회의" --date tomorrow --time 14:00 --category 업무 --memo "분기 목표 점검"
                  예: openlab add "운동하기" -d 오늘 -t 19:00 -c 건강
  list, 조회      일정 목록 조회
                  예: openlab start -s list                          (전체 조회)
                  예: openlab start -s list --filter today           (오늘 일정)
                  예: openlab start -s list --date 2026-11-10        (특정 일자 조회)
                  예: openlab start -s list --json                   (JSON 포맷 출력)
  done, 완료      일정을 완료(또는 조건부완료) 처리
                  예: openlab start -s done 1
  status, 상태    일정 상태 변경 (planned | in_progress | conditional_done | done)
                  예: openlab start -s status 1 in_progress
  update, 수정    일정 상세 정보 수정
                  예: openlab start -s update 1 --title "수정된 제목" --date 2026-11-15 --time 10:30
  delete, 삭제    일정 삭제
                  예: openlab start -s delete 1
  json, sync      로컬 {{date}}_schedule.json 파일 목록 조회 및 SQLite DB 양방향 동기화
                  예: openlab start -s json
  categories      카테고리(목록) 조회 / 추가 / 삭제
                  예: openlab start -s categories
                  예: openlab start -s categories add "프로젝트" --color "#AF52DE"
  summary, 요약   스마트 카운트 및 오늘의 일정 요약 출력
                  예: openlab start -s summary [--json]

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

    let normalized = s.replace(['/', '.'], "-");
    if let Ok(d) = NaiveDate::parse_from_str(&normalized, "%Y-%m-%d") {
        return Ok(d);
    }

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
        bail!("일정 제목을 입력해주세요.\n예: openlab start -s add \"팀 회의\" --date tomorrow --time 14:00 --category 업무");
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
    let json_path = paths::date_schedule_json_path(due_date);

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
                "json_file": json_path.to_string_lossy(),
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
        println!("  📄 JSON 저장됨: {}", json_path.display());
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
        bail!("조회할 일정 ID를 입력해주세요. 예: openlab start -s get 1");
    }
    let id: i64 = args[0]
        .trim_start_matches('#')
        .parse()
        .map_err(|_| anyhow!("올바른 일정 ID가 아닙니다: '{}'", args[0]))?;
    let todo = store
        .get(id)?
        .ok_or_else(|| anyhow!("ID #{id} 일정을 찾을 수 없습니다."))?;
    println!("{}", serde_json::to_string_pretty(&todo)?);
    Ok(())
}

fn cmd_done(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("완료 처리할 일정 ID를 입력해주세요. 예: openlab start -s done 1");
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
    let json_path = paths::date_schedule_json_path(updated.due_date);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "completed",
                "todo": updated,
                "json_file": json_path.to_string_lossy(),
            }))?
        );
    } else {
        println!(
            "✓ 일정 완료 처리됨 [ID: #{}] \"{}\" -> {}",
            updated.id,
            updated.title,
            updated.status.label()
        );
        println!("  📄 JSON 갱신됨: {}", json_path.display());
    }
    Ok(())
}

fn cmd_status(store: &Store, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        bail!("사용법: openlab start -s status <ID> <STATUS> [--condition <메모>]\n상태값: planned(진행예정), in_progress(진행중), conditional_done(조건부완료), done(완료)");
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
    let json_path = paths::date_schedule_json_path(updated.due_date);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "status_updated",
                "todo": updated,
                "json_file": json_path.to_string_lossy(),
            }))?
        );
    } else {
        println!(
            "✓ 상태 변경됨 [ID: #{}] \"{}\" -> {}",
            updated.id,
            updated.title,
            updated.status.label()
        );
        println!("  📄 JSON 갱신됨: {}", json_path.display());
    }
    Ok(())
}

fn cmd_update(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("수정할 일정 ID를 입력해주세요. 예: openlab start -s update 1 --title \"새 제목\" --date tomorrow");
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
    let json_path = paths::date_schedule_json_path(updated.due_date);

    if json_output {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "action": "updated",
                "todo": updated,
                "json_file": json_path.to_string_lossy(),
            }))?
        );
    } else {
        println!(
            "✓ 일정 수정 완료 [ID: #{}] \"{}\" (기한: {})",
            updated.id,
            updated.title,
            updated.due_date.format("%Y-%m-%d")
        );
        println!("  📄 JSON 갱신됨: {}", json_path.display());
    }
    Ok(())
}

fn cmd_delete(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("삭제할 일정 ID를 입력해주세요. 예: openlab start -s delete 1");
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
        println!(
            "✓ 일정 삭제 완료 [ID: #{}] \"{}\" (날짜: {}_schedule.json 반영됨)",
            id,
            t.title,
            t.due_date.format("%Y-%m-%d")
        );
    } else {
        println!("✓ 일정 삭제 완료 [ID: #{}]", id);
    }
    Ok(())
}

fn cmd_json(store: &Store, args: &[String]) -> Result<()> {
    let json_output = args.iter().any(|a| a == "--json");
    let _ = store.sync_all_json_files();
    let files = store.list_json_schedule_files()?;

    if json_output {
        let items: Vec<_> = files
            .iter()
            .map(|(date, path, count)| {
                json!({
                    "date": date,
                    "file": format!("{date}_schedule.json"),
                    "path": path.to_string_lossy(),
                    "count": count,
                })
            })
            .collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    println!("📂 일자별 JSON 저장 폴더: {}", paths::schedules_dir().display());
    if files.is_empty() {
        println!("  (저장된 {{date}}_schedule.json 파일이 없습니다)");
    } else {
        for (date, path, count) in files {
            println!(
                "  • {}_schedule.json ({}건) -> {}",
                date,
                count,
                path.display()
            );
        }
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
                bail!("추가할 카테고리 이름을 입력해주세요. 예: openlab start -s categories add \"프로젝트\"");
            }
            let name = &args[1];
            let mut color: u32 = 0x0071E3;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--color" | "-c" => {
                        i += 1;
                        if i < args.len() {
                            let hex_str =
                                args[i].trim_start_matches('#').trim_start_matches("0x");
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
                println!(
                    "✓ 카테고리 추가됨 [ID: #{}] \"{}\" (#{:06X})",
                    id, name, color
                );
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
        println!("📊 OpenLab / OpenSchedule 요약");
        println!(
            "  오늘(지연 포함): {}개 | 예정: {}개 | 전체 미완료: {}개 | 완료됨: {}개",
            counts.today, counts.scheduled, counts.all, counts.completed
        );
        println!("\n📌 오늘 및 지연된 일정:");
        print!("{}", store.context_text(DateFilter::TodayAndOverdue, 50)?);
    }
    Ok(())
}
