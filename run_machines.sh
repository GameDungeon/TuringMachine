#!/usr/bin/env bash

MAX_PARALLEL=6
FAIL_LOG="failed_machines.txt"

# Initialize output file with header
echo "# machine steps state symbol" >machine_output.txt

# Clear previous failure log
>"$FAIL_LOG"

run_machine() {
  local machine="$1"

  echo "Starting: $machine"

  if MACHINE="$machine" cargo run; then
    echo "SUCCESS: $machine"
  else
    echo "FAILED: $machine"
    echo "$machine" >>"$FAIL_LOG"
  fi
}

while IFS= read -r machine || [[ -n "$machine" ]]; do
  [[ -z "$machine" ]] && continue

  while (($(jobs -rp | wc -l) >= MAX_PARALLEL)); do
    wait -n
  done

  run_machine "$machine" &
done <machines.txt

wait

echo "All machines processed."
echo "Failed machines: $(wc -l <"$FAIL_LOG")"
