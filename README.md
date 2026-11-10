# OpenSchedule (OpenLab Schedule) v1.10

[![Version](https://img.shields.io/badge/version-1.10.0-0071e3?style=flat-square)](https://github.com/ksh6940/Open-Schdule)
[![License](https://img.shields.io/badge/License-Apache_2.0-34c759?style=flat-square)](./LICENSE)
[![Backend](https://img.shields.io/badge/Rust-Axum_%2B_SQLite-000000?style=flat-square&logo=rust)](https://www.rust-lang.org/)
[![Frontend](https://img.shields.io/badge/Electron-Apple_Design_System-47848F?style=flat-square&logo=electron)](https://www.electronjs.org/)

**OpenSchedule (OpenLab Schedule)** 은 **Apple Design System (Liquid Glass & HIG)** 기반의 프레임리스 데스크톱 일정·투두 관리 프로그램과 **상단 웹캠 핫존 드롭다운 오버레이**, 그리고 **Claude Code 스타일의 대화형 터미널 CLI (`openlab`)** 를 결합한 하이브리드 오픈소스 스케줄러입니다.

모든 일정 데이터는 **SQLite (`rusqlite`)** 데이터베이스와 로컬 **`schedules/{date}_schedule.json`** 파일에 실시간 양방향으로 저장·동기화되어, GUI 앱·터미널 CLI·외부 AI 에이전트(Claude Code, Cursor, Antigravity 등) 어디서든 끊김 없이 일정을 관리할 수 있습니다.

---

## ✨ 주요 특징 (v1.10)

### 1. 프레임리스 Apple Design System UI & 업그레이드된 UX
- **Frameless Liquid Glass 크롬**: OS 기본 제목 표시줄과 메뉴 표시줄을 제거하고, 드래그 이동 영역과 커스텀 윈도우 컨트롤(최소화·최대화/복원·닫기)이 통합된 53px 상단 Liquid Glass 네비게이션 바를 제공합니다.
- **실시간 달성률 프로그레스 바**: 현재 선택된 목록이나 날짜의 할 일 완료율(`%`)과 진행 현황을 헤더에서 즉시 확인할 수 있습니다.
- **플로팅 일정 추가 & 호버 퀵 액션**:
  - 오른쪽 아래 `+` 버튼이나 `Ctrl+N`으로 일정 추가 창을 열고, 날짜와 시간 등을 지정해 등록할 수 있습니다.
  - 각 일정 항목에 마우스를 올리면 **`[+1일 미루기]`**, **`[상태 순환 배지]`**, **`[즉시 삭제]`** 퀵 액션 버튼이 나타납니다.
- **플로팅 Liquid Glass 토스트 알림**: 일정 추가·완료·기한 변경·삭제·동기화 시 화면 하단에 부드러운 토스트 피드백이 표시됩니다.

### 2. `[투두리스트 | 캘린더]` 실시간 모드 전환
- **투두리스트 모드 (`Ctrl+1`)**:
  - **2x2 스마트 카드**: `오늘`, `예정`, `전체`, `완료됨` 실시간 카운트 필터
  - **내 목록 (카테고리)**: 커스텀 컬러 태그별 할 일 분류
  - **3단계 상태 관리**: 체크할 때마다 `○ 예정 (planned)` ➔ `◐ 진행중 (in_progress)` ➔ `● 완료 (done)` ➔ 예정으로 순환합니다. 기존 조건부 완료는 완료로 변환됩니다.
  - **일정 세부사항**: 오른쪽 패널에서 내용을 확인하고 일정 추가와 같은 창에서 수정합니다.
- **월간 캘린더 모드 (`Ctrl+2`)**:
  - 월간 달력에 각 날짜의 일정을 목록 색상으로 최대 두 개 표시합니다. 날짜를 클릭하면 오른쪽 패널에서 전체 일정을 확인할 수 있습니다.
  - 상단의 월 제목 옆 화살표로 이전 달과 다음 달을 이동할 수 있습니다.
  - 날짜 셀을 **한 번 클릭**하면 오른쪽 일정 패널이 열리고, **더블클릭**하면 해당 날짜의 상세 일정 추가 창이 열립니다. 일정을 클릭하면 수정 및 삭제할 수 있습니다.
  - 여러 날짜를 드래그하면 상세 입력 창에서 시작일과 종료일을 가진 하나의 일정으로 저장합니다.

### 3. 상단 웹캠 핫존 드롭다운 (Dynamic Island 오버레이)
- 화면 최상단 중앙(웹캠 영역)으로 마우스 커서를 이동하면 그림자 없는 오버레이 카드가 슬라이드 다운됩니다.
- 설정 화면에서 핫존 위치와 크기, 열기·닫기 지연, 오버레이 너비를 조정할 수 있습니다.
- 설정의 메모리 항목은 Electron 프로세스의 작업 집합 합계를 표시하며, 별도 Rust 엔진 메모리는 포함하지 않습니다. 임시 HTTP 캐시를 비울 수 있으며, 프로세스 메모리 감소를 보장하지는 않습니다. AI 데이터 관리와 AI 및 서드파티 서비스 연동은 아직 준비 중입니다.
- 설정 초기화는 일정과 목록을 유지합니다. 일정·목록과 설정을 모두 삭제하려면 앱과 엔진을 종료한 뒤 `openlab delete --data`를 실행하고 삭제 대상을 확인하세요.
- 상단 AI 버튼에서 대화 화면을 열 수 있습니다. 로컬 LLM 및 외부 도구 연동은 아직 구현되지 않아 메시지는 전송되지 않습니다.
- 일정 추가 및 수정에서 세부 할 일을 설정할 수 있습니다. 일정 알림 기능은 제거되었으며 기존 알림 데이터는 호환성을 위해 보존하지만 발송하지 않습니다.
- 일정 추가창의 달력에서 단일 날짜 또는 기간을 고르고, 별도의 시간 버튼에서 시간을 설정할 수 있습니다. 날짜를 선택하지 않으면 등록 시 오늘 날짜로 저장됩니다. 캘린더에서 추가할 때는 선택한 날짜 또는 기간이 미리 입력됩니다.
- 투두 항목을 왼쪽으로 밀면 제거, 오른쪽으로 밀면 수정할 수 있습니다. 짧게 밀면 해당 버튼이 나타나고, 더 길게 밀면 바로 실행됩니다. 상세 패널의 수정 버튼도 같은 일정 입력창을 엽니다.
- 둥근 모서리를 위한 투명 창은 유지하되 카드 배경은 불투명하게 렌더링하고, 블러와 그림자를 사용하지 않아 합성 부담을 줄입니다. 하드웨어 가속은 Electron 기본값을 따릅니다.
- 일정과 카테고리는 한 번에 조회하며, 내용이 바뀌지 않으면 목록을 다시 그리지 않습니다. 기존 엔진에 연결한 경우 기존 조회 API를 사용합니다.
- 설정 탭에서 기본 모니터 상단의 핫존 중심 위치, 너비, 높이, 대기 시간 및 활성화 여부를 저장할 수 있습니다. 한 번 열린 후에는 커서가 핫존을 벗어나야 다시 열립니다.
- 오버레이는 핫존 진입 시 미리 준비하고, 변경 작업은 서버 응답을 먼저 반영한 뒤 데이터 변경 알림을 모아 다시 조회합니다.
- 일정의 긴급도는 낮음(없음), ! 보통, !! 긴급, !!! 높음으로 지정합니다. 오버레이는 미완료 일정부터 긴급도 내림차순, 일정 날짜와 시간순으로 표시합니다.
- 작업 중인 창을 전환하지 않고도 오늘 할 일을 확인하고 상태를 변경할 수 있습니다.

### 4. Claude Code 스타일 인터랙티브 CLI (`openlab start -s`)
- 터미널에서 `openlab start -s`를 입력하면 Claude Code 감성의 대화형 셸이 실행됩니다.
- 슬래시 명령어(`/cal`, `/list`, `/today`, `/add`, `/done`, `/status`, `/edit`, `/delete`, `/cat`, `/sync`, `/program`)와 **한국어 자연어 입력**(예: `내일 오후 2시 팀 주간 회의 #업무`)을 모두 지원합니다.

### 5. SQLite ⇄ `schedules/{date}_schedule.json` 양방향 자동 저장소
- 기간 일정은 시작일의 JSON 파일에 한 번만 저장되며 `end_date`에 종료일을 기록합니다. 달력과 날짜별 조회에는 시작일부터 종료일까지 표시됩니다.
- 할 일을 추가·수정·삭제할 때마다 SQLite(`todos.db`)와 로컬 워크스페이스의 **`schedules/YYYY-MM-DD_schedule.json`** 파일이 동시에 자동 갱신됩니다.
- 배포된 Windows 앱은 워크스페이스 대신 `%APPDATA%\OpenSchedule\schedules`에 일정 JSON을 저장합니다.
- 외부 에디터나 AI 에이전트가 `{date}_schedule.json` 파일을 직접 생성·수정해도 파일 감시(`fs.watch`) 및 자동 병합 엔진이 이를 감지하여 앱 화면과 SQLite DB에 즉시 반영합니다.

---

## 🏗️ 시스템 아키텍처

```mermaid
flowchart LR
    subgraph Frontend["Electron Desktop GUI (Frameless v1.10)"]
        MainWin["메인 관리 윈도우\n(투두리스트 / 캘린더 뷰)"]
        OverlayWin["웹캠 핫존 드롭다운\n(Dynamic Island 오버레이)"]
    end

    subgraph CLI["OpenLab CLI (openlab.exe / openschedule.exe)"]
        InteractiveCLI["Claude Code 스타일 대화형 셸\n(openlab start -s)"]
        OneShotCLI["단일 명령어 & AI 연동\n(openlab start -s add/list/...)"]
    end

    subgraph Backend["Rust Core & Agent Engine"]
        AxumAPI["Axum 로컬 HTTP API\n(127.0.0.1 + Bearer Token)"]
        StoreEngine["Store 동기화 엔진\n(openschedule-core)"]
    end

    subgraph Storage["양방향 영구 저장소"]
        SQLite[("SQLite DB\n(%APPDATA%/OpenSchedule/todos.db)")]
        DateJSON[("일자별 JSON 파일\n(schedules/{date}_schedule.json)")]
    end

    MainWin <-->|"REST API / fs.watch"| AxumAPI
    OverlayWin <-->|"REST API / fs.watch"| AxumAPI
    AxumAPI <--> StoreEngine
    InteractiveCLI <--> StoreEngine
    OneShotCLI <--> StoreEngine
    StoreEngine <--> SQLite
    StoreEngine <--> DateJSON
```

---

## 🚀 시작하기 (설치 및 실행)

### 필수 요구 사항
- **Rust** (1.75 이상, `cargo`)
- **Node.js** (18 이상, `npm`)

### 1. 저장소 클론 및 패키지 설치

```powershell
git clone https://github.com/ksh6940/Open-Schdule.git
cd Open-Schdule

# Node.js 의존성 (Electron 및 아이콘) 설치
npm install

# Rust 릴리즈 바이너리 (.exe) 빌드
cargo build --release --workspace

# Windows 배포 ZIP 생성 (dist 폴더의 기존 버전은 덮어쓰지 않음)
.\scripts\package-windows.ps1
```

### 2. 실행 방법

터미널 어디서나 `openlab` 명령어로 CLI와 GUI 프로그램을 실행할 수 있습니다.

```powershell
# 1) Claude Code 스타일 대화형 CLI 실행 (Windows에서는 새 콘솔 창)
openlab start -s

# 2) 프레임리스 Electron GUI 데스크톱 프로그램 실행
openlab start -s program

# 또는 npm 스크립트로 GUI 직접 실행
npm start
```

Windows에서 대화형 `openlab start -s`는 새 콘솔을 엽니다. 단일 명령과
파이프로 입력을 전달하는 대화형 실행은 기존 터미널에서 동작합니다.
프로그램의 X 버튼은 창을 트레이로 숨기며, 트레이 아이콘을 클릭하면 다시 열립니다.
완전히 종료하려면 상단 연결 상태 버튼 또는 트레이 메뉴의 **종료**를 누르세요.
프로그램이 직접 시작한 백그라운드 엔진은 함께 종료되며, 별도로 실행 중이던 엔진은 유지됩니다.
중복 실행 시 기존 창이 열리며, 연결 상태 버튼이나 트레이 메뉴의 종료는 오버레이와 트레이도 종료합니다.

### 종료 및 삭제 명령 (Windows)

```powershell
openlab quit                         # 현재 설치의 GUI와 직접 시작한 엔진을 정상 종료
openlab quit --force                 # 현재 설치 경로에서 확인된 GUI와 엔진을 강제 종료
openlab delete --help
openlab delete --version 1.02        # dist의 OpenSchedule-v1.02-win32-x64 폴더 삭제
openlab delete --program "C:\경로\OpenSchedule-v1.02-win32-x64"
openlab delete --cache               # OpenSchedule 전용 Electron 캐시만 삭제
openlab delete --data                # 일정 JSON, DB, 설정 및 엔진 정보 삭제
```

`quit`은 별도로 실행한 엔진을 유지합니다. `--force`는 해당 설치 경로의 엔진도
강제 종료하며 저장하지 않은 작업이 손실될 수 있습니다. 구버전 GUI는 정상 종료
요청을 처리하지 못할 수 있으므로 필요한 경우 `--force`를 사용하세요.
삭제는 실행 중인 OpenSchedule GUI와 엔진을 모두 종료한 뒤 가능합니다. 대상 경로를
표시하고 `DELETE` 입력을 요구하며, 자동화에서는 `--yes`로 확인을 생략할 수 있습니다.
`--version`은 버전 또는 배포 폴더 경로를 받으며 `--program`은 배포 폴더 경로를
받습니다. 배포 폴더 전체가 삭제되므로 그 안의 개인 파일도 먼저 확인하세요.
Windows에서 실행 중인 CLI가 들어 있는 폴더는 삭제할 수 없으므로 다른 버전이나
프로젝트의 `openlab`으로 삭제하세요. ZIP 파일은 자동 삭제하지 않습니다.
데이터 삭제는 `%APPDATA%\OpenSchedule`의 데이터와 일정 JSON 및 존재하는 프로젝트의
`schedules` 일정 JSON을 지우므로 먼저 백업하세요. 개발 환경에서 다른 Electron 앱과
공유할 수 있는 `%APPDATA%\Electron` 캐시는 삭제하지 않습니다.
기존 일정 삭제 명령 `openlab delete <ID>`는 그대로 사용할 수 있습니다.

일정 JSON과 앱 데이터 폴더에 동일 날짜 파일이 모두 있으면 수정 시각이 더 최근인
파일을 가져와 DB와 다른 복사본에 반영합니다. 두 파일을 동시에 편집하면 마지막
수정본이 우선하므로 편집 전 백업하세요. 외부 JSON 변경은 실제 엔진의 일정 폴더와
앱 데이터 일정 폴더를 감시해 화면을 갱신합니다. JSON은 항목을 병합하므로 파일에서
항목을 제거하는 것만으로 DB의 해당 항목이 삭제되지는 않습니다.

### 3. 빌드된 `.exe` 실행 파일 위치

`cargo build --release --workspace` 및 패키징 실행 시 다음 경로에 독립 실행 파일(`.exe`)이 생성됩니다:

| 실행 파일 경로 | 설명 |
|---|---|
| `dist/openlab.exe` (`target/release/openlab.exe`) | 메인 통합 런처 (`openlab start -s` CLI 및 `openlab start -s program` GUI 실행) |
| `dist/openschedule.exe` (`target/release/openschedule.exe`) | 독립 CLI 전용 실행 파일 |
| `dist/openschedule-agent.exe` (`target/release/openschedule-agent.exe`) | 백그라운드 로컬 HTTP API 서버 & 에이전트 엔진 |

---

## 💻 CLI 사용 가이드 (`openlab`)

### 1. 대화형 셸 모드 (`openlab start -s`)

```text
╭─── OpenLab Schedule v1.10 ───────────────────────────────────────────────╮
│                                                                          │
│  ✻ Welcome to OpenLab Schedule CLI!                                      │
│                                                                          │
│    /help 명령어 도움말 · /cal 캘린더 뷰 · /list 투두리스트 · /program GUI   │
│    자연어 입력 예: 내일 오후 2시 프로젝트 미팅 #업무                     │
│                                                                          │
│    storage: SQLite + {date}_schedule.json                                │
╰──────────────────────────────────────────────────────────────────────────╯
```

- **슬래시 명령어**:
  - `/cal [YYYY-MM]` : 월간 캘린더 및 일자별 일정 표시
  - `/list [today|scheduled|all|completed|YYYY-MM-DD]` : 투두리스트 조회
  - `/add <제목> [-d 날짜] [-t 시간] [-c 카테고리] [-m 메모]` : 새 일정 추가
  - `/done <ID>` : 일정 완료 처리
  - `/status <ID> <next|planned|in_progress|done>` : 상태 순환 또는 변경
  - `/edit <ID> [--title ..] [--date ..] [--time ..]` : 일정 수정
  - `/settings show`, `/settings set <설정 이름> <값>` : 오버레이 설정 조회 및 수정
  - `/settings reset --yes` : 오버레이 등 앱 설정을 기본값으로 복원 (일정은 유지)
    - `/settings memory`, `/settings clear-http-cache --yes` : 실행 중인 GUI의 메모리 확인과 HTTP 캐시 정리
  - `/delete <ID>` : 일정 삭제
  - `/sync` : `schedules/{date}_schedule.json` ⇄ SQLite 수동 동기화
  - `/program` : Electron GUI 데스크톱 프로그램 즉시 실행

### 2. 단일 명령줄 실행 (AI 에이전트 & 스크립트 연동)

```powershell
# 새 일정 추가 (자동으로 schedules/{date}_schedule.json 생성 및 갱신)
openlab start -s add "팀 주간 회의" --date tomorrow --time 14:00 --category 업무 --memo "분기 목표 점검"

# 기간, 긴급도(0=낮음, 1=!, 2=!!, 3=!!!), 세부 할 일
openlab start -s add "프로젝트" --date 2030-01-01 --end-date 2030-01-03 --priority 3 --subtask "준비" --subtask "검토"
openlab start -s edit 1 --subtask-done 1 --subtask-remove 2 --clear-end-date --priority 0
openlab start -s status 1 next

# 설정 (settings set은 나머지 설정을 유지합니다)
openlab start -s settings show
openlab start -s settings set hotzone_enabled true
openlab start -s settings set hotzone_center_percent 50
openlab start -s settings set panel_width 480
# 지원 이름: hotzone_enabled, hotzone_width, hotzone_height, hotzone_center_percent,
# hotzone_dwell_ms, hide_delay_ms, panel_width. 초기화는 settings reset --yes

# 한국어 자연 날짜/시간 지정
openlab start -s add "저녁 유산소 운동" -d 오늘 -t "19:00" -c 건강

# 일정 조회 (테이블 출력 / AI 파싱용 JSON 출력)
openlab start -s list --filter today
openlab start -s list --date 2026-11-10
openlab start -s list --json

# 일정 상태 변경 / 수정 / 삭제
openlab start -s done 1
openlab start -s status 1 in_progress
openlab start -s edit 1 --title "수정된 회의" --date 2026-11-12 --time 15:30
openlab start -s delete 1
```

---

## ⌨️ GUI 단축키 모음

| 단축키 | 동작 |
|---|---|
| `Ctrl + N` | 새 일정 추가 모달 열기 |
| `Ctrl + 1` | **투두리스트 뷰**로 전환 |
| `Ctrl + 2` | **월간 캘린더 뷰**로 전환 |
| `Ctrl + S` | 일정 입력창 저장 또는 `schedules/{date}_schedule.json` 동기화 |
| `/` | 검색창 즉시 포커스 |
| `Esc` | 열려 있는 모달 또는 우측 인스펙터 패널 닫기 |
| **캘린더 날짜 클릭** | 해당 날짜의 일정 패널에서 조회 및 추가 |
| **일정 클릭** | 읽기 전용 세부보기를 열고, `수정` 버튼으로 편집 |

---

## 🗂️ 일자별 JSON 포맷 (`schedules/{date}_schedule.json`)

일정이 시작하는 날짜마다 `schedules/YYYY-MM-DD_schedule.json` 파일이 자동으로 생성·관리됩니다. 새로 가져온 일정에 `end_date`가 없으면 하루 일정으로 취급하고, 기존 일정의 수정 파일에 필드가 없으면 기존 기간을 유지합니다.

```json
{
  "date": "2026-11-10",
  "updated_at": "2026-11-10T14:00:00+09:00",
  "count": 1,
  "todos": [
    {
      "id": 1,
      "title": "팀 주간 회의",
      "memo": "분기 목표 점검",
      "due_date": "2026-11-10",
      "end_date": "2026-11-12",
      "due_time": "14:00",
      "status": "planned",
      "condition_note": "",
      "category_id": 2,
      "category_name": "업무",
      "created_at": "2026-11-10T09:00:00+09:00",
      "updated_at": "2026-11-10T14:00:00+09:00"
    }
  ]
}
```

---

## 👥 기여자 (Contributors)

- **[ksh6940](https://github.com/ksh6940)** - 프로젝트 기획, 아키텍처 설계 및 리드 개발
- **Gemini (Google DeepMind Antigravity)** - 페어 프로그래밍, 프레임리스 Apple Design System 및 Rust 코어/API 구현

---

## 📄 라이선스 (License)

본 프로젝트는 **Apache License, Version 2.0** 라이선스 하에 배포됩니다. 자세한 내용은 [LICENSE](./LICENSE) 파일을 참고하세요.

```text
Copyright 2026 ksh6940, Gemini (Google DeepMind Antigravity), and OpenSchedule Contributors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0
```
