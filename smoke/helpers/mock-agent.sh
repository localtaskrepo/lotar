#!/bin/bash
# Mock agent script that simulates Copilot CLI output format
# This allows testing agent job infrastructure without invoking actual LLMs

# Generate a session ID
SESSION_ID="test-session-$(date +%s)"

# Emit init event
echo '{"type":"system","subtype":"init","session_id":"'$SESSION_ID'"}'

# Sleep briefly to simulate processing
sleep 0.1

# Emit progress event
echo '{"type":"message","delta":true,"content":"Hello","session_id":"'$SESSION_ID'"}'

sleep 0.1

# Emit progress event
echo '{"type":"message","delta":true,"content":" from mock agent!","session_id":"'$SESSION_ID'"}'

sleep 0.1

# Emit final message
echo '{"type":"message","content":"Hello from mock agent!","session_id":"'$SESSION_ID'"}'

sleep 0.1

# Emit result event
echo '{"type":"result","result":"Completed successfully","session_id":"'$SESSION_ID'"}'

exit 0
