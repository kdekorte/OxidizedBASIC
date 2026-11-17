#!/bin/bash
# Launch the oxbasic IDE briefly to verify the window opens and stays alive.
cd "$(dirname "$0")/.."
./target/debug/oxbasic ide examples/clickme.bas examples/clickme.oxform > /tmp/ide_test.log 2>&1 &
PID=$!
sleep 10
if kill -0 $PID 2>/dev/null; then
  echo "IDE ALIVE (pid $PID)"
  kill $PID
  wait $PID 2>/dev/null
  exit 0
else
  echo "IDE EXITED EARLY"
  exit 1
fi
