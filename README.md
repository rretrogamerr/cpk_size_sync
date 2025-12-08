# cpk_size_sync

LEVEL5 게임의 `cpk_list.cfg.bin`을 언어 패치하면 생성되는 테이블의 파일 용량 정보를, 원본 `cpk_list.cfg.bin`에 덮어써 일관된 크기 정보를 만들어주는 작은 CLI 도구입니다.

## 요구 사항
- Rust 1.70+ (안정 채널이면 충분합니다)

## 사용법
- **개발 중**: 내부 빌드된 바이너리로 실행합니다.
  ```bash
  cargo run --release -- original.bin patched.bin synced.bin
  ```
- **배포판**: 배포된 실행 파일을 바로 사용합니다.
  ```bash
  cpk_file_size_sync original.bin patched.bin synced.bin
  ```

인수 설명:
- `original.bin`: 크기를 갱신할 원본 CPK 테이블
- `patched.bin`: 크기 정보가 올바르게 들어 있는 패치 테이블
- `synced.bin`: 출력 파일 경로 (필수)

디버그 로그가 필요하면 실행 전에 `CPK_DEBUG=1`을 설정하세요.
