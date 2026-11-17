#!/bin/bash
# Launch the oxbasic GUI briefly to verify the window opens and stays alive.
cd "$(dirname "$0")/.."
./target/debug/oxbasic gui examples/clickme.oxform examples/clickme.bas > /tmp/gui_test.log 2>&1 &
PID=$!
sleep 15
if kill -0 $PID 2>/dev/null; then
  echo "WINDOW ALIVE (pid $PID)"
  kill $PID
  wait $PID 2>/dev/null
  exit 0
else
  echo "PROCESS EXITED EARLY"
  exit 1
fi
