# WTG quick validation harness
#
# Usage:
#   Live visible run:
#     .\wtg_sink.ps1
#
#   Capture to file when needed:
#     .\wtg_sink.ps1 > wtg_validation_output.txt 2>&1
#
# Behavior:
#   - Starts each WTG command.
#   - Waits 2 seconds.
#   - Kills wtg.exe if it is still running.
#   - Moves to the next command.
#   - Only one wtg.exe instance is expected to be running.

$commands = @(
    "--help",
    "--version",
    "--once",
    "--once --stats",
    "--once --sink jsonl",
    "--once --sink csv",
    "--once --stats --sink jsonl",
    "--once --stats --sink csv",
    "--watch --interval 1000",
    "--watch --stats --interval 1000",
    "--watch --interval 1000 --sink jsonl",
    "--watch --interval 1000 --sink csv",
    "--watch --stats --interval 1000 --sink jsonl",
    "--watch --stats --interval 1000 --sink csv",
    "--probe",
    "--probe --sink jsonl",
    "--probe --sink csv",
    "--probe-fields --field-id 74",
    "--probe-fields --field-id 74 --sink jsonl",
    "--probe-fields --field-id 74 --sink csv",
    "--probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95",
    "--probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95 --sink jsonl",
    "--probe-fields --field-id 74 --field-id 78 --field-id 83 --field-id 94 --field-id 95 --sink csv"
)

foreach ($cmd in $commands) {
    ">>> .\wtg.exe $cmd"
    cmd /c "start /b .\wtg.exe $cmd & timeout 2 & taskkill /F /IM wtg.exe"
}