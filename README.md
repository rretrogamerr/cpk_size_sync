# cpk_size_sync

Small CLI that keeps file-size metadata in Level-5 `cpk_list.cfg.bin` tables consistent after applying a language patch.

## What it does
- Reads two versions of the table: an original `cpk_list.cfg.bin` and a patched one that already has correct sizes.
- Extracts `CPK_ITEM` entries keyed by the path parts (first two string fields).
- Takes the patched size value (3rd value, index `2`) for entries without a suffix and maps it to the matching entry in the original file.
- Writes the size into the original file’s primary size field (5th value, index `4`), preserving the original integer width so the table layout stays intact.
- Outputs a synchronized table where every size field matches the patched data while all other metadata remains untouched.

Use it when a modded table has good size information but you need to keep the original structure and checksums elsewhere in the file.

## Requirements
- Rust 1.70+ (stable channel is fine)

## Usage
- Development build:
  ```bash
  cargo run --release -- original.bin patched.bin synced.bin
  ```
- Released binary:
  ```bash
  cpk_file_size_sync original.bin patched.bin synced.bin
  ```

Arguments:
- `original.bin`: Source table whose size fields will be updated.
- `patched.bin`: Patched table that contains the correct size values.
- `synced.bin`: Output path for the synchronized table (required).

Notes:
- `-h`/`--help` shows CLI help, `-v`/`--version` prints the version.
- Set `CPK_DEBUG=1` to print parsed entry details while running.
