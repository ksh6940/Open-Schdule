//! OpenSchedule 독립 CLI 실행 파일 (`openschedule`).
//! `openlab start -s` / `openlab start -s program` 및 CLI 하위 명령을 동일하게 지원합니다.

use openschedule_core::cli::{launch_interactive_cli, parse_launch_action, run_cli, LaunchAction};
use openschedule_core::paths;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let res = match parse_launch_action(&args) {
        LaunchAction::RunInteractiveCli => launch_interactive_cli(),
        LaunchAction::RunCliCommand(cmd_args) => run_cli(&cmd_args),
        LaunchAction::RunGui => {
            let gui_exe = paths::sibling_exe("openlab");
            std::process::Command::new(gui_exe)
                .args(["start", "-s", "program"])
                .spawn()
                .map(|_| ())
                .map_err(|e| anyhow::anyhow!("GUI 실행 실패: {e}"))
        }
    };

    if let Err(e) = res {
        eprintln!("오류: {e}");
        std::process::exit(1);
    }
}
