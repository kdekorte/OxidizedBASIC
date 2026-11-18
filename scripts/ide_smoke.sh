#!/bin/bash
# Launch the oxbasic IDE briefly to verify the window opens and stays alive.
# With OXBASIC_IDE_AUTORUN=1 it also auto-runs the program so a screenshot
# can confirm the Run pipeline produces output in the output pane.
cd "$(dirname "$0")/.."
if [ "$1" = "--capture" ]; then
    OXBASIC_IDE_AUTORUN=1 ./target/debug/oxbasic ide examples/clickme.bas examples/clickme.oxform > /tmp/ide_test.log 2>&1 &
else
    ./target/debug/oxbasic ide examples/clickme.bas examples/clickme.oxform > /tmp/ide_test.log 2>&1 &
fi
PID=$!
if [ "$1" = "--capture" ]; then
    sleep 5
    screencapture -x /tmp/ide_screenshot.png
    sleep 1
else
    sleep 10
fi
if kill -0 $PID 2>/dev/null; then
  echo "IDE ALIVE (pid $PID)"
  kill $PID
  wait $PID 2>/dev/null
  exit 0
else
  echo "IDE EXITED EARLY"
  cat /tmp/ide_test.log
  exit 1
fi
