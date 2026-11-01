#!/usr/bin/env bash

MAX_PARALLEL=8
FAIL_LOG="failed_machines.txt"
OUTPUT_FILE="machine_output.txt"

# Initialize output file
echo "# machine steps state symbol" >"$OUTPUT_FILE"

# Clear previous failure log
>"$FAIL_LOG"

# Count machines (excluding empty lines)
TOTAL=$(grep -cve '^[[:space:]]*$' machines.txt)

COMPLETED=0
RUNNING=0

run_machine() {
  local machine="$1"

  if ! MACHINE="$machine" cargo run >/dev/null 2>&1; then
    echo "$machine" >>"$FAIL_LOG"
  fi
}

show_progress() {
  local percent=$((COMPLETED * 100 / TOTAL))
  local filled=$((percent / 2))
  local empty=$((50 - filled))

  printf "\r[%s%s] %d%% (%d/%d)" \
    "$(printf '%*s' "$filled" '' | tr ' ' '#')" \
    "$(printf '%*s' "$empty" '')" \
    "$percent" "$COMPLETED" "$TOTAL"
}

while IFS= read -r machine || [[ -n "$machine" ]]; do
  [[ -z "$machine" ]] && continue

  while ((RUNNING >= MAX_PARALLEL)); do
    wait -n
    ((COMPLETED++))
    ((RUNNING--))
    show_progress
  done

  run_machine "$machine" &
  ((RUNNING++))
done <machines.txt

# Wait for remaining processes
while ((RUNNING > 0)); do
  wait -n
  ((COMPLETED++))
  ((RUNNING--))
  show_progress
done

echo
echo "All machines processed."
echo "Successful: $((TOTAL - $(wc -l <"$FAIL_LOG")))"
echo "Failed: $(wc -l <"$FAIL_LOG")"
