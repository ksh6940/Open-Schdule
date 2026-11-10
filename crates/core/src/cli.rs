//! OpenSchedule / OpenLab CLI 모듈 (Claude Code CLI 스타일 인터랙티브 셸 & 명령어).
//! - `openlab start -s`         : Claude Code 스타일 인터랙티브 CLI 실행
//! - `openlab start -s program` : GUI 데스크톱 프로그램 실행
//! - `openlab start -s <명령>`  : 단일 CLI 명령 실행 (AI 에이전트 & 스크립트 연동)

use std::io::{self, Write};

use anyhow::{anyhow, bail, Result};
#[cfg(windows)]
use anyhow::Context;
use chrono::{Datelike, Duration, Local, NaiveDate, NaiveTime};
use serde_json::json;

use crate::{
    model::{Status, Todo, TodoDraft},
    paths, DateFilter, Store, TodoQuery,
};

// ── Claude Code ANSI 컬러 팔레트 ──
const C_RESET: &str = "\x1b[0m";
const C_BOLD: &str = "\x1b[1m";
const C_DIM: &str = "\x1b[90m";
/// Claude 시그니처 코랄/테라코타 오렌지
const C_CLAUDE: &str = "\x1b[38;5;209m";
/// 포인트 블루
const C_BLUE: &str = "\x1b[38;5;75m";
/// 완료 그린
const C_GREEN: &str = "\x1b[38;5;78m";
/// 진행중/예정 앰버
const C_YELLOW: &str = "\x1b[38;5;214m";
/// 조건부완료 퍼플
const C_PURPLE: &str = "\x1b[38;5;141m";
/// 경고/삭제 레드
const C_RED: &str = "\x1b[38;5;203m";
/// 밝은 화이트
const C_WHITE: &str = "\x1b[97m";

/// Windows 터미널에서 UTF-8 및 ANSI 이스케이프 컬러 활성화
pub fn enable_ansi_support() {
    #[cfg(windows)]
    {
        extern "system" {
            fn GetStdHandle(nStdHandle: u32) -> isize;
            fn GetConsoleMode(hConsoleHandle: isize, lpMode: *mut u32) -> i32;
            fn SetConsoleMode(hConsoleHandle: isize, dwMode: u32) -> i32;
            fn SetConsoleOutputCP(wCodePageID: u32) -> i32;
            fn SetConsoleCP(wCodePageID: u32) -> i32;
        }
        const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
        const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
        unsafe {
            SetConsoleOutputCP(65001);
            SetConsoleCP(65001);
            let handle = GetStdHandle(STD_OUTPUT_HANDLE);
            if handle != 0 && handle != -1 {
                let mut mode: u32 = 0;
                if GetConsoleMode(handle, &mut mode) != 0 {
                    let _ = SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING);
                }
            }
        }
    }
}

/// 실행 모드 판별 결과
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchAction {
    /// GUI 프로그램 실행 (`openlab start -s program` 또는 인자 없음)
    RunGui,
    /// Claude Code 스타일 대화형 CLI 실행 (`openlab start -s`)
    RunInteractiveCli,
    /// 단일 CLI 명령 실행 (`openlab start -s <cmd>` 또는 `openschedule <cmd>`)
    RunCliCommand(Vec<String>),
}

/// 커맨드라인 인자를 분석하여 GUI 실행 / CLI 실행을 판별합니다.
pub fn parse_launch_action(raw_args: &[String]) -> LaunchAction {
    if raw_args.is_empty() {
        return LaunchAction::RunGui;
    }

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

#[cfg(test)]
mod launch_tests {
    use super::{parse_launch_action, LaunchAction};

    #[test]
    fn interactive_and_one_shot_commands_remain_distinct() {
        let args = |items: &[&str]| items.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert_eq!(
            parse_launch_action(&args(&["start", "-s"])),
            LaunchAction::RunInteractiveCli
        );
        assert_eq!(
            parse_launch_action(&args(&["start", "-s", "list", "--json"])),
            LaunchAction::RunCliCommand(args(&["list", "--json"]))
        );
        assert_eq!(
            parse_launch_action(&args(&["start", "-s", "program"])),
            LaunchAction::RunGui
        );
    }
}

/// Windows에서 대화형 세션은 별도 콘솔로 열고, 파이프 입력은 현재 터미널에서 처리합니다.
pub fn launch_interactive_cli() -> Result<()> {
    #[cfg(windows)]
    {
        use std::io::IsTerminal;
        use std::os::windows::process::CommandExt;

        const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;
        const CHILD_MARKER: &str = "OPENSCHEDULE_INTERACTIVE_CONSOLE";
        if std::env::var_os(CHILD_MARKER).is_none() && io::stdin().is_terminal() {
            std::process::Command::new(std::env::current_exe()?)
                .args(std::env::args_os().skip(1))
                .env(CHILD_MARKER, "1")
                .creation_flags(CREATE_NEW_CONSOLE)
                .spawn()
                .context("새 CLI 터미널을 열지 못했습니다")?;
            return Ok(());
        }
    }
    run_interactive_cli()
}

/// CLI 서브커맨드인지 판별합니다.
pub fn is_cli_invocation(args: &[String]) -> bool {
    !matches!(parse_launch_action(args), LaunchAction::RunGui)
}

/// Claude Code 스타일 웰컴 박스 출력
fn print_claude_banner(store: &Store) {
    let counts = store.smart_counts().unwrap_or_default();
    let json_files = store.list_json_schedule_files().unwrap_or_default();
    let sched_dir = paths::schedules_dir();
    let today_str = Local::now().format("%Y-%m-%d").to_string();

    println!();
    println!(
        "{C_CLAUDE}╭───{C_RESET} {C_BOLD}{C_WHITE}OpenLab Schedule{C_RESET} {C_DIM}v{}{C_RESET} {C_CLAUDE}──────────────────────────────────────────────╮{C_RESET}",
        env!("CARGO_PKG_VERSION")
    );
    println!(
        "{C_CLAUDE}│{C_RESET}                                                                          {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}  {C_CLAUDE}✻{C_RESET} {C_BOLD}{C_WHITE}Welcome to OpenLab Schedule CLI!{C_RESET}                                      {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}                                                                          {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}/help{C_RESET} 명령어 도움말 · {C_DIM}/cal{C_RESET} 캘린더 뷰 · {C_DIM}/list{C_RESET} 투두리스트 · {C_DIM}/program{C_RESET} GUI   {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}자연어 입력 예:{C_RESET} {C_BLUE}내일 오후 2시 프로젝트 미팅 #업무{C_RESET}                     {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}                                                                          {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}storage:{C_RESET} SQLite + {C_WHITE}{{date}}_schedule.json{C_RESET} ({} files in schedules/)",
        json_files.len()
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}path:{C_RESET}    {}",
        sched_dir.display()
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}today:{C_RESET}   {today_str} · {C_BLUE}오늘 {}개{C_RESET} · {C_YELLOW}예정 {}개{C_RESET} · {C_WHITE}전체 {}개{C_RESET} · {C_GREEN}완료 {}개{C_RESET}",
        counts.today, counts.scheduled, counts.all, counts.completed
    );
    println!(
        "{C_CLAUDE}╰──────────────────────────────────────────────────────────────────────────╯{C_RESET}"
    );
}

/// `openlab start -s` 입력 시 실행되는 Claude Code 스타일 인터랙티브 CLI
pub fn run_interactive_cli() -> Result<()> {
    enable_ansi_support();
    let store = Store::open_default()?;
    print_claude_banner(&store);

    // 오늘 할 일이 있으면 시작 시 브리핑 표시
    let today_todos = store
        .query(&TodoQuery {
            date: DateFilter::TodayAndOverdue,
            ..Default::default()
        })
        .unwrap_or_default();
    if !today_todos.is_empty() {
        println!();
        println!(
            "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Today({}){C_RESET}",
            Local::now().format("%Y-%m-%d")
        );
        render_todo_box(&store, &today_todos);
    }

    let stdin = io::stdin();
    loop {
        println!();
        println!(
            "{C_DIM}────────────────────────────────────────────────────────────────────────────{C_RESET}"
        );
        print!("{C_CLAUDE}{C_BOLD}❯{C_RESET} ");
        io::stdout().flush().ok();

        let mut line = String::new();
        let bytes = stdin.read_line(&mut line)?;
        if bytes == 0 {
            println!();
            break;
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        // 종료 명령어
        if matches!(
            trimmed.to_lowercase().as_str(),
            "exit" | "quit" | "q" | "/exit" | "/quit" | "/q" | "종료" | "/종료" | "0"
        ) {
            println!(
                "\n{C_CLAUDE}⏺{C_RESET} {C_DIM}세션을 종료합니다. (일정 데이터는 SQLite 및 schedules/{{date}}_schedule.json에 안전하게 저장되었습니다){C_RESET}"
            );
            break;
        }

        // 화면 지우기
        if matches!(
            trimmed.to_lowercase().as_str(),
            "/clear" | "clear" | "/cls" | "cls"
        ) {
            print!("\x1b[2J\x1b[1;1H");
            io::stdout().flush().ok();
            print_claude_banner(&store);
            continue;
        }

        // GUI 프로그램 실행
        if matches!(
            trimmed.to_lowercase().as_str(),
            "/program" | "program" | "/gui" | "gui"
        ) {
            let exe = paths::sibling_exe("openlab");
            println!(
                "\n{C_CLAUDE}⏺{C_RESET} {C_BOLD}Launch(openlab start -s program){C_RESET}"
            );
            match std::process::Command::new(&exe)
                .args(["start", "-s", "program"])
                .spawn()
            {
                Ok(_) => {
                    println!(
                        "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ GUI 데스크톱 프로그램이 실행되었습니다.{C_RESET}"
                    );
                }
                Err(e) => {
                    println!(
                        "  {C_DIM}⎿{C_RESET}  {C_RED}✗ GUI 실행 실패: {e}{C_RESET}"
                    );
                }
            }
            continue;
        }

        // 도움말 단축키
        if trimmed == "?" || trimmed.eq_ignore_ascii_case("/help") || trimmed.eq_ignore_ascii_case("help") {
            print_help();
            continue;
        }

        let mut tokens = split_cli_tokens(trimmed);
        if tokens.is_empty() {
            continue;
        }

        // 앞에 `openlab`, `openschedule`, `start -s`가 붙은 경우 제거
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

        // 선행 슬래시(`/add` -> `add`) 제거 지원
        let first_cmd = tokens[0].trim_start_matches('/').to_lowercase();
        if is_known_command(&first_cmd) {
            tokens[0] = first_cmd;
            if let Err(e) = execute_with_store(&store, &tokens) {
                println!(
                    "\n{C_RED}⏺ Error{C_RESET}\n  {C_DIM}⎿{C_RESET}  {C_RED}{e}{C_RESET}"
                );
            }
        } else {
            // 명령어가 아니면 Claude Code처럼 자연어 문장으로 해석해 바로 할 일 추가!
            if let Err(e) = handle_natural_language_add(&store, trimmed) {
                println!(
                    "\n{C_RED}⏺ Error{C_RESET}\n  {C_DIM}⎿{C_RESET}  {C_RED}{e}{C_RESET}"
                );
            }
        }
    }

    Ok(())
}

fn is_known_command(cmd: &str) -> bool {
    matches!(
        cmd,
        "add"
            | "ass"
            | "ad"
            | "create"
            | "new"
            | "추가"
            | "등록"
            | "list"
            | "ls"
            | "l"
            | "todo"
            | "todos"
            | "목록"
            | "조회"
            | "today"
            | "오늘"
            | "cal"
            | "calendar"
            | "달력"
            | "캘린더"
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
            | "del"
            | "rm"
            | "remove"
            | "삭제"
            | "categories"
            | "cats"
            | "cat"
            | "category"
            | "카테고리"
            | "summary"
            | "counts"
            | "요약"
            | "sync"
            | "json"
            | "load"
            | "동기화"
            | "help"
            | "--help"
            | "-h"
    )
}

/// 자연어 입력(예: "내일 오후 2시 팀 주간 회의 #업무 // 분기 목표 점검")을 파싱해 일정 등록
fn handle_natural_language_add(store: &Store, input: &str) -> Result<()> {
    // 만약 입력에 `-d`, `-t`, `-c`, `-m` 등 CLI 플래그가 포함되어 있다면 `cmd_add` 파서로 위임
    let raw_tokens = split_cli_tokens(input);
    if raw_tokens.iter().any(|t| {
        matches!(
            t.as_str(),
            "-d" | "--date" | "-t" | "--time" | "-c" | "--category" | "-m" | "--memo"
        )
    }) {
        let mut cleaned = raw_tokens;
        if cleaned
            .first()
            .map(|s| s.starts_with('/'))
            .unwrap_or(false)
        {
            cleaned.remove(0);
        }
        return cmd_add(store, &cleaned);
    }

    let today = Local::now().date_naive();
    let mut due_date = today;
    let mut due_time: Option<NaiveTime> = None;
    let mut category_name: Option<String> = None;

    // `//` 또는 `--memo` 뒤의 내용은 메모로 분리
    let (main_part, memo) = if let Some((left, right)) = input.split_once("//") {
        (left.trim(), right.trim().to_string())
    } else {
        (input.trim(), String::new())
    };

    let words: Vec<&str> = main_part.split_whitespace().collect();
    let mut title_words: Vec<String> = Vec::new();
    let mut i = 0;

    while i < words.len() {
        let w = words[i];

        // #카테고리 태그 인식
        if let Some(cat) = w.strip_prefix('#') {
            if !cat.is_empty() {
                category_name = Some(cat.to_string());
                i += 1;
                continue;
            }
        }

        // 날짜 단어 인식 ("오늘", "내일", "모레", "다음주", "YYYY-MM-DD")
        if matches!(w, "오늘" | "내일" | "모레" | "다음주" | "today" | "tomorrow")
            || (w.len() == 10 && w.chars().filter(|&c| c == '-').count() == 2)
        {
            if let Ok(d) = parse_flexible_date(w) {
                due_date = d;
                i += 1;
                continue;
            }
        }

        // "오전/오후 N시 [M분]" 2~3단어 시간 인식
        if matches!(w, "오전" | "오후") && i + 1 < words.len() && words[i + 1].contains('시') {
            let mut combined = format!("{} {}", w, words[i + 1]);
            let mut step = 2;
            if i + 2 < words.len() && words[i + 2].ends_with('분') {
                combined.push(' ');
                combined.push_str(words[i + 2]);
                step = 3;
            }
            if let Ok(Some(t)) = parse_flexible_time(&combined) {
                due_time = Some(t);
                i += step;
                continue;
            }
        }

        // "14:00" 또는 "15시" 단일 단어 시간 인식
        if (w.contains(':') && w.len() <= 5) || w.ends_with('시') {
            if let Ok(Some(t)) = parse_flexible_time(w) {
                due_time = Some(t);
                i += 1;
                continue;
            }
        }

        // 끝에 붙은 "추가", "추가해줘", "등록" 제거
        if i == words.len() - 1 && matches!(w, "추가" | "추가해줘" | "등록" | "등록해줘") && !title_words.is_empty() {
            i += 1;
            continue;
        }

        title_words.push(w.to_string());
        i += 1;
    }

    let title = title_words.join(" ").trim().to_string();
    if title.is_empty() {
        bail!("할 일 제목을 인식하지 못했습니다. `/help`를 입력해 사용법을 확인하세요.");
    }

    let category_id = match category_name {
        Some(ref c) => resolve_category(store, c, true)?,
        None => None,
    };

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
    let todo = store
        .get(id)?
        .ok_or_else(|| anyhow!("생성된 일정을 불러오지 못했습니다."))?;
    let cats = store.categories()?;
    let cat_name = todo
        .category_id
        .and_then(|cid| cats.iter().find(|c| c.id == cid).map(|c| c.name.as_str()))
        .unwrap_or("미분류");

    print_claude_todo_added(&todo, cat_name);
    Ok(())
}

pub fn run_cli(args: &[String]) -> Result<()> {
    enable_ansi_support();
    if crate::management::is_management_command(args) {
        return crate::management::run(args);
    }
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h" | "/help") {
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
    if args.is_empty() || matches!(args[0].as_str(), "help" | "--help" | "-h" | "/help") {
        print_help();
        return Ok(());
    }

    let cmd = args[0].trim_start_matches('/');
    let rest = &args[1..];

    match cmd {
        "add" | "ass" | "ad" | "create" | "new" | "추가" | "등록" => cmd_add(store, rest),
        "list" | "ls" | "l" | "todo" | "todos" | "목록" | "조회" => cmd_list(store, rest),
        "today" | "오늘" => {
            let mut v = vec!["--filter".to_string(), "today_and_overdue".to_string()];
            v.extend_from_slice(rest);
            cmd_list(store, &v)
        }
        "cal" | "calendar" | "달력" | "캘린더" => cmd_calendar(store, rest),
        "get" | "show" => cmd_get(store, rest),
        "done" | "complete" | "완료" => cmd_done(store, rest),
        "status" | "상태" => cmd_status(store, rest),
        "update" | "edit" | "수정" => cmd_update(store, rest),
        "delete" | "del" | "rm" | "remove" | "삭제" => cmd_delete(store, rest),
        "categories" | "cats" | "cat" | "category" | "카테고리" => cmd_categories(store, rest),
        "summary" | "counts" | "요약" => cmd_summary(store, rest),
        "sync" | "json" | "load" | "동기화" => cmd_json_sync(store, rest),
        other => {
            bail!("알 수 없는 명령어입니다: '{other}'\n사용법을 보려면 `/help` 또는 `openlab start -s help`를 실행하세요.");
        }
    }
}

pub fn print_help() {
    println!();
    println!(
        "{C_CLAUDE}╭───{C_RESET} {C_BOLD}{C_WHITE}OpenLab Schedule CLI Commands{C_RESET} {C_CLAUDE}────────────────────────────────────────╮{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}  {C_BOLD}실행 명령어 (일반 터미널){C_RESET}                                               {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_CLAUDE}openlab start -s{C_RESET}                 Claude Code 스타일 인터랙티브 CLI    {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_CLAUDE}openlab start -s program{C_RESET}         GUI 데스크톱 프로그램 실행           {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_CLAUDE}openlab start -s <명령> [옵션]{C_RESET}   단일 명령 즉시 실행 (AI 연동)        {C_CLAUDE}│{C_RESET}"
    );
    println!("    openlab quit [--force]     현재 설치의 GUI/엔진 종료");
    println!("    openlab delete --help      버전, 프로그램, 캐시, 데이터 삭제");
    println!(
        "{C_CLAUDE}├──────────────────────────────────────────────────────────────────────────┤{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}  {C_BOLD}슬래시 & CLI 명령어 (인터랙티브 셸에서 `/` 생략 가능){C_RESET}                   {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/add{C_RESET} <제목> [-d 날짜] [-t 시간] [-c 목록] [-m 메모]  새 할 일 추가    {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/list{C_RESET} [today|scheduled|all|completed|YYYY-MM-DD]     할 일 목록 조회  {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/today{C_RESET}                                               오늘 할 일 조회  {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/cal{C_RESET} [YYYY-MM]                                       월간 캘린더 뷰   {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/done{C_RESET} <ID> [--condition <사유>]                      완료 처리        {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/status{C_RESET} <ID> <planned|in_progress|conditional_done|done> 상태 변경    {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/edit{C_RESET} <ID> [--title ..] [-d ..] [-t ..] [-c ..]      할 일 수정       {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/delete{C_RESET} <ID>                                         할 일 삭제       {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/cat{C_RESET} [add <이름> --color #HEX | delete <ID>]         카테고리 관리    {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/sync{C_RESET}                                                JSON ⇄ SQL 동기화{C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_BLUE}/program{C_RESET} · {C_BLUE}/clear{C_RESET} · {C_BLUE}/exit{C_RESET}                            GUI실행/초기화/끝{C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}├──────────────────────────────────────────────────────────────────────────┤{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}  {C_BOLD}자연어 빠른 입력 (명령어 없이 프롬프트에 바로 입력){C_RESET}                     {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}❯{C_RESET} 내일 오후 2시 팀 주간 회의 #업무 // 분기 목표 점검                  {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}│{C_RESET}    {C_DIM}❯{C_RESET} 오늘 19:00 저녁 유산소 운동 #건강                                   {C_CLAUDE}│{C_RESET}"
    );
    println!(
        "{C_CLAUDE}╰──────────────────────────────────────────────────────────────────────────╯{C_RESET}"
    );
}

fn status_colored(st: Status) -> String {
    match st {
        Status::Planned => format!("{C_DIM}{} {}{C_RESET}", st.icon(), st.label()),
        Status::InProgress => format!("{C_YELLOW}{} {}{C_RESET}", st.icon(), st.label()),
        Status::ConditionalDone => format!("{C_PURPLE}{} {}{C_RESET}", st.icon(), st.label()),
        Status::Done => format!("{C_GREEN}{} {}{C_RESET}", st.icon(), st.label()),
    }
}

fn print_claude_todo_added(todo: &Todo, cat_name: &str) {
    let json_name = format!("schedules/{}_schedule.json", todo.due_date.format("%Y-%m-%d"));
    let time_str = todo
        .due_time
        .map(|t| format!(" {}", t.format("%H:%M")))
        .unwrap_or_default();
    println!();
    println!(
        "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Write({json_name}){C_RESET}"
    );
    println!(
        "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 일정 등록 완료{C_RESET} {C_BOLD}[ID: #{}]{C_RESET} \"{}\"",
        todo.id, todo.title
    );
    println!(
        "     {C_DIM}기한:{C_RESET} {}{} · {C_DIM}목록:{C_RESET} {C_BLUE}{cat_name}{C_RESET} · {C_DIM}상태:{C_RESET} {}",
        todo.due_date.format("%Y-%m-%d"),
        time_str,
        status_colored(todo.status)
    );
    if !todo.memo.trim().is_empty() {
        println!("     {C_DIM}메모:{C_RESET} {}", todo.memo.trim());
    }
}

fn render_todo_box(store: &Store, todos: &[Todo]) {
    let cats = store.categories().unwrap_or_default();
    let today = Local::now().date_naive();

    println!(
        "  {C_DIM}⎿{C_RESET}  총 {C_BOLD}{}{C_RESET}개의 일정",
        todos.len()
    );
    println!(
        "     {C_DIM}╭────────────────────────────────────────────────────────────────────╮{C_RESET}"
    );
    for t in todos {
        let cat_name = t
            .category_id
            .and_then(|cid| cats.iter().find(|c| c.id == cid).map(|c| c.name.as_str()))
            .unwrap_or("미분류");
        let time_str = t
            .due_time
            .map(|tm| format!(" {}", tm.format("%H:%M")))
            .unwrap_or_else(|| "      ".to_string());
        let overdue_badge = if t.is_overdue(today) {
            format!(" {C_RED}[기한초과]{C_RESET}")
        } else {
            String::new()
        };
        println!(
            "     {C_DIM}│{C_RESET} {C_BOLD}#{:<3}{C_RESET} {}  {}{}{}  {C_BLUE}[{}]{C_RESET} {}",
            t.id,
            status_colored(t.status),
            t.due_date.format("%Y-%m-%d"),
            time_str,
            overdue_badge,
            cat_name,
            t.title
        );
        if !t.condition_note.trim().is_empty() {
            println!(
                "     {C_DIM}│      ⎿{C_RESET} {C_PURPLE}조건: {}{C_RESET}",
                t.condition_note.trim()
            );
        }
        if !t.memo.trim().is_empty() {
            println!(
                "     {C_DIM}│      ⎿ 메모: {}{C_RESET}",
                t.memo.trim()
            );
        }
    }
    println!(
        "     {C_DIM}╰────────────────────────────────────────────────────────────────────╯{C_RESET}"
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
            arg if arg.starts_with('#') && arg.len() > 1 => {
                category_arg = Some(arg.trim_start_matches('#').to_string());
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
        bail!("일정 제목을 입력해주세요.\n예: /add \"팀 회의\" -d 내일 -t 14:00 -c 업무");
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
        print_claude_todo_added(&todo, cat_name);
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

    if json_output {
        println!("{}", serde_json::to_string_pretty(&todos)?);
        return Ok(());
    }

    let filter_label = match filter {
        DateFilter::Day(d) => format!("schedules/{}_schedule.json", d.format("%Y-%m-%d")),
        DateFilter::Today | DateFilter::TodayAndOverdue => "Today".to_string(),
        DateFilter::Scheduled => "Scheduled".to_string(),
        DateFilter::Completed => "Completed".to_string(),
        DateFilter::ThisWeek => "ThisWeek".to_string(),
        _ => "All Schedules".to_string(),
    };

    println!();
    println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}Read({filter_label}){C_RESET}");
    if todos.is_empty() {
        println!("  {C_DIM}⎿  조회된 일정이 없습니다.{C_RESET}");
        return Ok(());
    }

    render_todo_box(store, &todos);
    Ok(())
}

/// Claude Code 스타일 터미널 월간 캘린더 뷰 (`/cal [YYYY-MM]`)
fn cmd_calendar(store: &Store, args: &[String]) -> Result<()> {
    let today = Local::now().date_naive();
    let (year, month) = if let Some(arg) = args.first() {
        let parts: Vec<&str> = arg.split(['-', '/', '.']).collect();
        if parts.len() >= 2 {
            let y = parts[0].parse::<i32>().unwrap_or(today.year());
            let m = parts[1].parse::<u32>().unwrap_or(today.month()).clamp(1, 12);
            (y, m)
        } else if let Ok(m) = arg.parse::<u32>() {
            (today.year(), m.clamp(1, 12))
        } else {
            (today.year(), today.month())
        }
    } else {
        (today.year(), today.month())
    };

    let month_todos = store.todos_by_month(year, month)?;
    let first_day = NaiveDate::from_ymd_opt(year, month, 1).unwrap_or(today);
    let start_offset = first_day.weekday().num_days_from_sunday() as i64;
    let grid_start = first_day - Duration::days(start_offset);

    println!();
    println!(
        "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Calendar({year}년 {month:02}월){C_RESET}"
    );
    println!(
        "  {C_DIM}⎿{C_RESET}  {C_DIM}╭────────┬────────┬────────┬────────┬────────┬────────┬────────╮{C_RESET}"
    );
    println!(
        "     {C_DIM}│{C_RESET} {C_RED}  일   {C_RESET}{C_DIM}│{C_RESET}   월   {C_DIM}│{C_RESET}   화   {C_DIM}│{C_RESET}   수   {C_DIM}│{C_RESET}   목   {C_DIM}│{C_RESET}   금   {C_DIM}│{C_RESET} {C_BLUE}  토   {C_RESET}{C_DIM}│{C_RESET}"
    );
    println!(
        "     {C_DIM}├────────┼────────┼────────┼────────┼────────┼────────┼────────┤{C_RESET}"
    );

    for week in 0..6 {
        let mut row_str = format!("     {C_DIM}│{C_RESET}");
        for dow in 0..7 {
            let d = grid_start + Duration::days(week * 7 + dow);
            let in_month = d.month() == month;
            let is_today = d == today;
            let cnt = month_todos.get(&d).map(|v| v.len()).unwrap_or(0);

            let cell_text = if !in_month {
                format!("{C_DIM}  {:2}    {C_RESET}", d.day())
            } else if is_today && cnt > 0 {
                format!("{C_CLAUDE}{C_BOLD}[{:2}]{C_RESET}{C_GREEN}+{:<2}{C_RESET} ", d.day(), cnt)
            } else if is_today {
                format!("{C_CLAUDE}{C_BOLD}[{:2}]   {C_RESET}", d.day())
            } else if cnt > 0 {
                format!("{C_WHITE} {:2}{C_RESET} {C_GREEN}•{:<2}{C_RESET} ", d.day(), cnt)
            } else if dow == 0 {
                format!("{C_RED}  {:2}    {C_RESET}", d.day())
            } else if dow == 6 {
                format!("{C_BLUE}  {:2}    {C_RESET}", d.day())
            } else {
                format!("  {:2}    ", d.day())
            };

            row_str.push_str(&cell_text);
            row_str.push_str(&format!("{C_DIM}│{C_RESET}"));
        }
        println!("{row_str}");
    }
    println!(
        "     {C_DIM}╰────────┴────────┴────────┴────────┴────────┴────────┴────────╯{C_RESET}"
    );

    // 이번 달 일정 목록 함께 표시
    let mut flat_todos: Vec<Todo> = month_todos.into_values().flatten().collect();
    flat_todos.sort_by_key(|t| (t.due_date, t.due_time));
    if !flat_todos.is_empty() {
        render_todo_box(store, &flat_todos);
    } else {
        println!(
            "     {C_DIM}{year}년 {month:02}월에 등록된 일정이 없습니다.{C_RESET}"
        );
    }

    Ok(())
}

fn cmd_get(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("조회할 일정 ID를 입력해주세요. 예: /get 1");
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
        bail!("완료 처리할 일정 ID를 입력해주세요. 예: /done 1");
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
        println!();
        println!(
            "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Update(schedules/{}_schedule.json){C_RESET}",
            updated.due_date.format("%Y-%m-%d")
        );
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 일정 완료 처리됨{C_RESET} {C_BOLD}[ID: #{}]{C_RESET} \"{}\" -> {}",
            updated.id,
            updated.title,
            status_colored(updated.status)
        );
    }
    Ok(())
}

fn cmd_status(store: &Store, args: &[String]) -> Result<()> {
    if args.len() < 2 {
        bail!("사용법: /status <ID> <STATUS> [--condition <메모>]\n상태값: planned(진행예정), in_progress(진행중), conditional_done(조건부완료), done(완료)");
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
        println!();
        println!(
            "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Update(schedules/{}_schedule.json){C_RESET}",
            updated.due_date.format("%Y-%m-%d")
        );
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 상태 변경됨{C_RESET} {C_BOLD}[ID: #{}]{C_RESET} \"{}\" -> {}",
            updated.id,
            updated.title,
            status_colored(updated.status)
        );
    }
    Ok(())
}

fn cmd_update(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("수정할 일정 ID를 입력해주세요. 예: /edit 1 --title \"새 제목\" --date tomorrow");
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
        println!();
        println!(
            "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Update(schedules/{}_schedule.json){C_RESET}",
            updated.due_date.format("%Y-%m-%d")
        );
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 일정 수정 완료{C_RESET} {C_BOLD}[ID: #{}]{C_RESET} \"{}\" (기한: {})",
            updated.id,
            updated.title,
            updated.due_date.format("%Y-%m-%d")
        );
    }
    Ok(())
}

fn cmd_delete(store: &Store, args: &[String]) -> Result<()> {
    if args.is_empty() {
        bail!("삭제할 일정 ID를 입력해주세요. 예: /delete 1");
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
        println!();
        println!(
            "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Delete(schedules/{}_schedule.json){C_RESET}",
            t.due_date.format("%Y-%m-%d")
        );
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 일정 삭제 완료{C_RESET} {C_BOLD}[ID: #{}]{C_RESET} \"{}\"",
            id, t.title
        );
    } else {
        println!();
        println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}Delete(#{id}){C_RESET}");
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 일정 삭제 완료{C_RESET} {C_BOLD}[ID: #{}]{C_RESET}",
            id
        );
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
            println!();
            println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}Categories({}개){C_RESET}", cats.len());
            for c in cats {
                let cnt = counts.get(&c.id).copied().unwrap_or(0);
                println!(
                    "  {C_DIM}⎿{C_RESET}  [#{:<2}] {C_BLUE}{}{C_RESET} {C_DIM}(#{:06X}) · 미완료 {}개{C_RESET}",
                    c.id, c.name, c.color, cnt
                );
            }
        }
        return Ok(());
    }

    match args[0].as_str() {
        "add" | "create" | "추가" => {
            if args.len() < 2 {
                bail!("추가할 카테고리 이름을 입력해주세요. 예: /cat add \"프로젝트\"");
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
                println!();
                println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}AddCategory({name}){C_RESET}");
                println!(
                    "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 카테고리 추가됨{C_RESET} [ID: #{id}] \"{name}\" (#{color:06X})"
                );
            }
        }
        "delete" | "rm" | "삭제" => {
            if args.len() < 2 {
                bail!("삭제할 카테고리 ID를 입력해주세요.");
            }
            let id: i64 = args[1].trim_start_matches('#').parse()?;
            store.delete_category(id)?;
            println!();
            println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}DeleteCategory(#{id}){C_RESET}");
            println!(
                "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 카테고리 삭제 완료{C_RESET} [ID: #{id}]"
            );
        }
        _ => bail!("알 수 없는 categories 하위 명령입니다: '{}'", args[0]),
    }

    Ok(())
}

fn cmd_json_sync(store: &Store, _args: &[String]) -> Result<()> {
    let synced = store.sync_all_json_files()?;
    let files = store.list_json_schedule_files()?;
    println!();
    println!(
        "{C_CLAUDE}⏺{C_RESET} {C_BOLD}Sync(SQLite ⇄ schedules/{{date}}_schedule.json){C_RESET}"
    );
    println!(
        "  {C_DIM}⎿{C_RESET}  {C_GREEN}✓ 동기화 완료{C_RESET} ({synced}개 파일 병합됨, 총 {}개 일자별 JSON 파일)",
        files.len()
    );
    for (date, path, count) in files {
        println!(
            "     • {C_WHITE}{date}_schedule.json{C_RESET} ({count}건) {C_DIM}-> {}{C_RESET}",
            path.display()
        );
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
        println!();
        println!("{C_CLAUDE}⏺{C_RESET} {C_BOLD}Summary(OpenLab Schedule){C_RESET}");
        println!(
            "  {C_DIM}⎿{C_RESET}  {C_BLUE}오늘(지연 포함): {}개{C_RESET} · {C_YELLOW}예정: {}개{C_RESET} · {C_WHITE}전체 미완료: {}개{C_RESET} · {C_GREEN}완료됨: {}개{C_RESET}",
            counts.today, counts.scheduled, counts.all, counts.completed
        );
        if !today_todos.is_empty() {
            render_todo_box(store, &today_todos);
        }
    }
    Ok(())
}
