# OpenSchedule / OpenLab (오픈스케줄)

> **순수 Rust 네이티브 데스크톱 프로그램 (`eframe`/`egui` + Apple Light Mode & Phosphor Icons) + SQLite & `{date}_schedule.json` 양방향 스토리지 + AI/터미널 CLI (`openlab`)**
>
> - 🍎 **애플 라이트 모드 (Apple Light Mode)**: Fog Canvas (`#F5F5F7`), Pure White Surface (`#FFFFFF`), Apple Blue (`#0071E3`), 한국어 시스템 폰트 & 내장 [Phosphor Icons](https://phosphoricons.com/) 디자인 시스템
> - 🗓️ **[캘린더 뷰 / 투두리스트] 실시간 모드 전환**: 상단 세그먼트 탭으로 월간 캘린더 그리드 뷰와 3단 투두리스트 뷰를 즉시 전환
> - 🗂️ **SQLite + 로컬 `{date}_schedule.json` 양방향 자동 관리**: 할 일을 생성·수정·삭제할 때마다 SQL(`rusqlite` SQLite) 엔진으로 쿼리/집계함과 동시에 로컬 `schedules/{date}_schedule.json` 파일로 일자별 자동 저장 및 불러오기 지원
> - 🏝️ **웹캠 핫존 드롭다운 (Dynamic Island Light Glass)**: 화면 상단 웹캠 영역에 마우스를 올리면 라이트 글래스 투두 드롭다운이 즉시 열림
> - 💻 **어느 터미널에서나 작동하는 `openlab` 런처**:
>   - `openlab start -s` ➔ 대화형 CLI 일정 관리 프로그램 실행
>   - `openlab start -s program` ➔ GUI 데스크톱 프로그램 실행
>   - `openlab start -s <명령어>` ➔ 외부 AI 에이전트 및 터미널 원라이너 명령 즉시 실행

---

## 🚀 실행 방법 (어느 경로의 일반 터미널에서든 실행 가능)

```powershell
# 1. 대화형 CLI 일정 관리 모드 실행
openlab start -s

# 2. GUI 데스크톱 프로그램 (투두리스트 / 캘린더 뷰 + 상단 웹캠 핫존 오버레이) 실행
openlab start -s program
```

---

## 🌟 주요 기능 및 화면 구성

### 1. [투두리스트 / 캘린더 뷰 / 데이터 & CLI] 모드 전환 (`crates/agent/src/ui_main.rs`)

- **📋 투두리스트 모드 (`ViewMode::TodoList`)**
  - **좌측 사이드바**: 2x2 스마트 카드 그리드 (`오늘`, `예정`, `전체`, `완료됨`) 및 커스텀 컬러 팔레트 기반 **내 목록(카테고리)** 관리
  - **중앙 할 일 목록**:
    - 제목, 기한(`오늘`, `내일`, `다음주`, `직접입력`), 시간(`09:00`, `14:00`, `18:00`, `직접입력`), 카테고리, 메모를 지원하는 **인라인 빠른 일정 추가 카드**
    - 원형 체크박스 완료 토글 및 4단계 상태 배지 (`진행예정` ➔ `진행중` ➔ `조건부완료` ➔ `완료`)
  - **우측 세부 인스펙터 패널**:
    - 선택한 일정의 제목, 상태, 조건부 사유, 기한 날짜(`{date}_schedule.json` 자동 이동 반영), 시간, 카테고리, 메모 **즉시 수정 및 삭제**

- **📅 캘린더 뷰 모드 (`ViewMode::Calendar`)**
  - **월간 7열 캘린더 그리드**:
    - 이전 달 / 다음 달 / `[오늘로 이동]` 네비게이션
    - 각 날짜 셀에 등록된 할 일의 완료/전체 배지(`완료/전체`)와 일정 타이틀 미리보기 표시
    - 날짜 클릭 시 해당 일자(`YYYY-MM-DD`) 선택 및 우측 패널 연동
  - **선택 날짜 전용 투두 패널 (`{date}_schedule.json`)**:
    - 선택한 날짜의 JSON 파일명 배지 표시 및 해당 날짜로 즉시 할 일 추가
    - 선택 날짜의 할 일 완료 토글, 상태 변경, 상세 수정 인스펙터, 삭제 지원

- **🗄️ 데이터 & CLI 뷰 (`ViewMode::Settings`)**
  - 로컬 `schedules/{date}_schedule.json` 파일 목록, 파일별 할 일 개수, 용량 실시간 확인
  - **`[JSON ⇄ SQLite 전체 동기화/불러오기]`** 버튼으로 외부에서 수정/추가된 `{date}_schedule.json` 파일을 SQLite DB로 즉시 반영
  - 웹캠 상단 핫존 크기/유지 시간 조절 및 미리보기 테스트

---

### 2. 일자별 JSON (`{date}_schedule.json`) + SQLite 하이브리드 저장소 (`crates/core/src/store.rs`)

- 모든 할 일 데이터는 SQLite(`openschedule.db`)를 통해 빠르게 필터링·정렬·집계되며, 동시에 일자별 JSON 파일인 **`schedules/{date}_schedule.json`** (예: `schedules/2026-11-10_schedule.json`)로 자동 저장됩니다.
- 날짜가 변경되면 이전 날짜의 JSON 파일과 새 날짜의 JSON 파일이 모두 자동으로 갱신되며, 해당 날짜의 모든 일정이 삭제되면 빈 JSON 파일도 깔끔하게 정리됩니다.
- 사용자가 직접 `{date}_schedule.json` 파일을 편집하거나 추가한 경우에도 프로그램 시작 시 또는 `sync` 명령 실행 시 SQLite DB로 자동 병합(Import)됩니다.

```json
{
  "date": "2026-11-10",
  "updated_at": "2026-11-10 14:00:00",
  "total_count": 1,
  "completed_count": 0,
  "todos": [
    {
      "id": 1,
      "title": "팀 주간 회의",
      "memo": "분기 목표 점검",
      "due_date": "2026-11-10",
      "due_time": "14:00",
      "status": "planned",
      "status_label": "진행예정",
      "condition_note": "",
      "category_id": 2,
      "category_name": "업무",
      "category_color": "#FF9500",
      "created_at": "2026-11-10 09:00:00",
      "completed_at": null
    }
  ]
}
```

---

### 3. CLI 명령줄 도구 (`openlab start -s`)

대화형 메뉴는 물론, 단일 명령줄 실행도 완벽히 지원합니다.

```powershell
# 대화형 CLI 실행 (메뉴 1~7번 선택 또는 직접 명령어 입력)
openlab start -s

# GUI 프로그램 실행
openlab start -s program

# 1. 일정 추가 (자동으로 schedules/{date}_schedule.json 저장)
openlab start -s add "팀 주간 회의" --date 2026-11-10 --time 14:00 --category 업무 --memo "분기 목표 점검"

# 2. 일정 목록 조회 (전체 / 오늘 / 특정 날짜 / JSON 포맷)
openlab start -s list
openlab start -s list --date 2026-11-10
openlab start -s list --json

# 3. 일정 수정 / 상태 변경 / 완료 / 삭제 (SQLite + {date}_schedule.json 즉시 반영)
openlab start -s update 1 --title "수정된 회의" --date 2026-11-11 --time 15:30
openlab start -s status 1 in_progress
openlab start -s done 1
openlab start -s delete 1

# 4. 로컬 {date}_schedule.json 파일 목록 확인 및 양방향 동기화
openlab start -s json
```
