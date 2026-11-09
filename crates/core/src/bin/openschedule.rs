//! OpenSchedule 독립 CLI 실행 파일 (`openschedule`).
//! AI 에이전트 또는 터미널에서 직접 호출하여 일정을 추가/조회/완료/수정/삭제합니다.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Err(e) = openschedule_core::cli::run_cli(&args) {
        eprintln!("오류: {e}");
        std::process::exit(1);
    }
}

