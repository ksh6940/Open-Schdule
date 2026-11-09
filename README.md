# OpenSchedule (오픈스케줄)

> **고성능 Rust 백엔드 엔진 + Apple Reminders 디자인 시스템 & Phosphor Icons 기반 Electron 데스크톱 앱 + AI 연동 CLI**
> 
> - 🍎 **애플 라이트 모드 (Apple Light Mode)**: Fog Canvas (`#f5f5f7`), Pure White Surface (`#ffffff`), Apple Blue (`#0071e3`), SF Pro 폰트 & [Phosphor Icons](https://phosphoricons.com/) 디자인 시스템
> - ⚡ **Rust 엔진 기반 초고속 SQLite 스토어**: 투두 CRUD, 4대 상태 순환, 마감일/시간 관리, 실시간 스마트 카운트 집계
> - 🏝️ **웹캠 핫존 드롭다운 (Dynamic Island Light Glass)**: 화면 상단 웹캠 마우스 호버 시 라이트 글래스 투두 카드 슬라이드 다운
> - 💻 **AI 에이전트 & 터미널 연동 CLI (`openschedule`)**: 외부 AI(Claude Code, Antigravity, Cursor 등) 또는 터미널에서 명령줄로 일정을 즉시 추가·조회·완료하면 앱과 핫존 오버레이에 실시간 반영

---

## 🌟 주요 기능 및 화면 구성

### 1. 메인 관리 앱 (`electron/renderer/index.html`)
- **좌측 사이드바: 2x2 스마트 카드 그리드 & 내 목록 (My Lists)**
  - 🔵 **오늘 (Today)**: 블루 캘린더 배지 + 실시간 잔여 할 일 카운트
  - 🟠 **예정 (Scheduled)**: 오렌지 시계 배지 + 미래 기한 카운트
  - ⚫ **전체 (All)**: 다크 트레이 배지 + 전체 활성 할 일 카운트
  - 🟢 **완료됨 (Completed)**: 그린 체크 배지 + 완료된 할 일 카운트
  - **내 목록**: 커스텀 카테고리 색상 칩과 실시간 카운트, 6가지 애플 컬러 팔레트 기반 **`+ 목록 추가`** 모달
- **일정 추가 기능 (2가지 방식 지원)**:
  - **인라인 일정 빠른 추가 바**: 제목 입력과 함께 기한 날짜, 시간, 카테고리 목록, 메모를 한 줄에서 즉시 지정하고 `Enter` 또는 `[+ 일정 추가]` 버튼으로 등록
  - **`+ 새 일정 추가` 모달 (`Ctrl + N`)**: 상단 네비게이션의 `[+ 새 일정 추가]` 버튼이나 `Ctrl+N` 단축키로 열리며, 빠른 날짜 칩(`오늘`, `내일`, `다음 주`)과 빠른 시간 칩(`09:00`, `14:00`, `18:00`), 카테고리, 초기 상태, 상세 메모를 입력해 등록
- **중앙 메인 목록**:
  - Apple Display 32px 대형 볼드 타이틀 (선택된 카테고리 고유 색상 자동 반영)
  - **애플 원형 체크박스 토글**: 미완료 링 `○` ➔ 클릭 시 부드러운 애니메이션과 함께 완료 `✓` 토글
  - **4대 세부 상태 알약 (`진행예정`, `진행중`, `조건부완료`, `완료`)**: 원클릭으로 세부 상태 순환
  - 기한 초과 시 Apple Danger Red (`#ff3b30`) 경고 및 조건부 완료 사유 배지 표시
- **우측 세부사항 인스펙터 패널**:
  - 할 일 클릭 시 슬라이드로 열리는 HIG 18px 카드
  - 제목, 메모, **원터치 날짜 칩 (`[오늘]`, `[내일]`, `[다음 주]`)**, **원터치 시간 칩 (`[09:00]`, `[13:00]`, `[18:00]`, `[시간 삭제]`)**

---

### 2. 노트북 웹캠 상단 핫존 드롭다운 (`electron/renderer/overlay.html`)
- **다이내믹 아일랜드 & 라이트 글래스 (Light Mode Glass)**:
  - 마우스를 노트북 웹캠(화면 상단 중앙)으로 가져가면 부드럽게 슬라이드 다운
  - 오늘 일자 캘린더 배지, 오늘의 미리알림 요일/날짜, 실시간 진척도 알약 (`3/5 완료 (60%)`)
  - **오버레이 내부 빠른 일정 추가 (제목 + 시간 + Enter/추가 버튼)** 탑재
  - 마우스가 벗어나면 350ms 후 자동으로 닫힘

---

### 3. CLI 명령줄 도구 (`openschedule` / `node cli.js`) — AI 에이전트 연동
외부 AI 어시스턴트나 터미널에서 직접 일정을 추가·조회·완료할 수 있으며, SQLite WAL 감지를 통해 실행 중인 Electron 앱 및 핫존 오버레이에 즉시 반영됩니다.

```powershell
# 1. 일정 추가 (날짜, 시간, 카테고리, 메모 지정)
.\target\debug\openschedule.exe add "팀 주간 회의" --date tomorrow --time 14:00 --category 업무 --memo "분기 목표 점검"

# 한국어 자연 날짜/시간 지원 (오늘, 내일, 모레, 다음주, +3d / 오전 9시, 오후 7시 등)
.\target\debug\openschedule.exe add "저녁 유산소 운동" -d 오늘 -t "오후 7시" -c 건강

# Node/NPM 런처로도 동일하게 실행 가능
node cli.js add "코드 리뷰" -d 내일 -t 11:00 -c 업무

# 2. 일정 목록 조회 (일반 출력 / AI 파싱용 JSON 출력)
.\target\debug\openschedule.exe list --filter today
.\target\debug\openschedule.exe list --json

# 3. 일정 완료 / 상태 변경 / 수정 / 삭제
.\target\debug\openschedule.exe done 1
.\target\debug\openschedule.exe status 1 in_progress
.\target\debug\openschedule.exe update 1 --title "수정된 회의" --time 15:30
.\target\debug\openschedule.exe delete 1

# 4. 오늘의 요약 브리핑
.\target\debug\openschedule.exe summary
```

---

## 🚀 실행 방법

```powershell
npm start
```
`npm start`를 실행하면 Electron이 백그라운드에서 Rust 엔진(`openschedule-agent.exe --headless`)을 자동 기동하고 메인 관리 앱과 상단 웹캠 핫존 드롭다운을 활성화합니다.
