#!/bin/bash

# LoTaR Integration Test Script
# Tests the actual compiled binary with real commands

set -e  # Exit on any error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Test configuration
TEST_DIR=$(mktemp -d)
BINARY_PATH="$(pwd)/target/release/lotar"  # Use absolute path
PASSED=0
FAILED=0
TOTAL=0

echo -e "${BLUE}🚀 LoTaR Integration Test Suite${NC}"
echo -e "${BLUE}================================${NC}"
echo "Test directory: $TEST_DIR"

# Cleanup function
cleanup() {
    echo -e "\n${YELLOW}🧹 Cleaning up...${NC}"
    cd "$OLDPWD" 2>/dev/null || true
    rm -rf "$TEST_DIR" 2>/dev/null || true
    echo -e "${GREEN}✅ Cleanup complete${NC}"
}

# Set trap to cleanup on exit
trap cleanup EXIT

# Test helper functions
run_test() {
    local test_name="$1"
    local test_command="$2"
    local expected_exit_code="${3:-0}"
    local expected_output="$4"

    TOTAL=$((TOTAL + 1))
    echo -e "\n${BLUE}Test $TOTAL: $test_name${NC}"
    echo "Command: $test_command"

    # Run the command and capture output and exit code
    if output=$(cd "$TEST_DIR" && eval "$test_command" 2>&1); then
        actual_exit_code=0
    else
        actual_exit_code=$?
    fi

    # Check exit code
    if [ "$actual_exit_code" -eq "$expected_exit_code" ]; then
        echo -e "${GREEN}✅ Exit code: $actual_exit_code (expected: $expected_exit_code)${NC}"
        exit_code_ok=true
    else
        echo -e "${RED}❌ Exit code: $actual_exit_code (expected: $expected_exit_code)${NC}"
        exit_code_ok=false
    fi

    # Check output if expected output is provided
    output_ok=true
    if [ -n "$expected_output" ]; then
        if echo "$output" | grep -q "$expected_output"; then
            echo -e "${GREEN}✅ Output contains: '$expected_output'${NC}"
        else
            echo -e "${RED}❌ Output missing: '$expected_output'${NC}"
            echo -e "${YELLOW}Actual output:${NC}"
            echo "$output"
            output_ok=false
        fi
    fi

    # Update counters
    if [ "$exit_code_ok" = true ] && [ "$output_ok" = true ]; then
        PASSED=$((PASSED + 1))
        echo -e "${GREEN}✅ PASSED${NC}"
    else
        FAILED=$((FAILED + 1))
        echo -e "${RED}❌ FAILED${NC}"
        echo -e "${YELLOW}Output:${NC}"
        echo "$output"
    fi
}

# Build the binary first
echo -e "\n${YELLOW}🔨 Building release binary...${NC}"
if cargo build --release; then
    echo -e "${GREEN}✅ Build successful${NC}"
else
    echo -e "${RED}❌ Build failed${NC}"
    exit 1
fi

# Verify binary exists
if [ ! -f "$BINARY_PATH" ]; then
    echo -e "${RED}❌ Binary not found at $BINARY_PATH${NC}"
    exit 1
fi

echo -e "${GREEN}✅ Binary found: $BINARY_PATH${NC}"

# Start integration tests
echo -e "\n${BLUE}🧪 Starting Integration Tests${NC}"
echo -e "${BLUE}==============================${NC}"

# Test 1: Help command
run_test "Help Command" "$BINARY_PATH help" 0 "LoTaR"

# Test 2: Invalid command
run_test "Invalid Command" "$BINARY_PATH invalid_command" 1 "Invalid command"

# Test 3: No command
run_test "No Command" "$BINARY_PATH" 1 "No command specified"

# Test 4: Task creation
run_test "Create Task" "$BINARY_PATH task add --title='Test Task 1' --priority=2 --project=test-project" 0 "Added task with id"

# Test 5: Another task creation with tags
run_test "Create Task with Tags" "$BINARY_PATH task add --title='Test Task 2' --tag=urgent --tag=bug --project=test-project" 0 "Added task with id"

# Test 6: List tasks
run_test "List Tasks" "$BINARY_PATH task list --project=test-project" 0 "Listing tasks for project"

# Test 7: Search tasks
run_test "Search Tasks" "$BINARY_PATH task search 'Test' --project=test-project" 0 "Searching for"

# Test 8: Edit task (first create a task and get its ID)
TASK_OUTPUT=$(cd "$TEST_DIR" && $BINARY_PATH task add --title="Task to Edit" --project=test-project 2>&1)
TASK_ID=$(echo "$TASK_OUTPUT" | sed -n 's/Added task with id: \(.*\)/\1/p')
echo "Debug: Task creation output: $TASK_OUTPUT"
echo "Debug: Extracted task ID: $TASK_ID"

if [ -z "$TASK_ID" ]; then
    echo -e "${RED}❌ Failed to extract task ID from output: $TASK_OUTPUT${NC}"
    FAILED=$((FAILED + 1))
    TOTAL=$((TOTAL + 1))
else
    # Remove --project parameter since task ID already contains project info
    run_test "Edit Task" "$BINARY_PATH task edit $TASK_ID --description='Updated description'" 0 "updated successfully"
fi

# Test 9: Update task status (use the same task ID)
if [ -n "$TASK_ID" ]; then
    # Remove --project parameter since task ID already contains project info
    run_test "Update Task Status" "$BINARY_PATH task status $TASK_ID IN_PROGRESS" 0 "status updated"
else
    echo -e "${RED}❌ Skipping status update test - no valid task ID${NC}"
    FAILED=$((FAILED + 1))
    TOTAL=$((TOTAL + 1))
fi

# Test 10: Search with filters
run_test "Search with Status Filter" "$BINARY_PATH task search 'Task' --status=IN_PROGRESS --project=test-project" 0 "Searching for"

# Test 11: Config operations
run_test "Config Set" "$BINARY_PATH config set test_key test_value" 0 "Setting test_key to test_value"

# Test 12: Scan command (create a test file first)
echo "// TODO: This is a test todo comment" > "$TEST_DIR/test.js"
run_test "Scan for TODOs" "$BINARY_PATH scan $TEST_DIR" 0 "Scanning"

# Test 13: Serve command (start and quickly stop)
run_test "Serve Command Start" "timeout 2s $BINARY_PATH serve 8080 || true" 0 "Listening on port"

# Error handling tests
echo -e "\n${BLUE}🚨 Error Handling Tests${NC}"
echo -e "${BLUE}========================${NC}"

# Test 14: Invalid task ID - Update expected message since we no longer validate numeric IDs
run_test "Invalid Task ID" "$BINARY_PATH task edit invalid_id --title='New Title'" 1 "not found"

# Test 15: Non-existent task ID
run_test "Non-existent Task ID" "$BINARY_PATH task edit 99999 --title='New Title'" 1 "not found"

# Test 16: Missing title
run_test "Missing Title" "$BINARY_PATH task add --priority=1" 1 "Title is required"

# Test 17: Non-existent directory scan
run_test "Non-existent Directory Scan" "$BINARY_PATH scan /nonexistent/path" 1 "does not exist"

# Test 18: Config missing arguments
run_test "Config Missing Arguments" "$BINARY_PATH config" 1 "No config operation specified"

# File structure verification tests
echo -e "\n${BLUE}📁 File Structure Tests${NC}"
echo -e "${BLUE}========================${NC}"

# Test 19: Verify .tasks directory was created
if [ -d "$TEST_DIR/.tasks" ]; then
    PASSED=$((PASSED + 1))
    echo -e "${GREEN}✅ .tasks directory created${NC}"
else
    FAILED=$((FAILED + 1))
    echo -e "${RED}❌ .tasks directory not created${NC}"
fi
TOTAL=$((TOTAL + 1))

# Test 20: Verify project directory exists (check for mapped folder name)
project_folder=""
if [ -f "$TEST_DIR/.tasks/project_mappings.yml" ]; then
    # Extract the folder name for test-project from mappings
    project_folder=$(grep "test-project:" "$TEST_DIR/.tasks/project_mappings.yml" | cut -d' ' -f2 | tr -d '\n\r')
fi

# If no mapping found, fallback to checking for any project folder
if [ -z "$project_folder" ]; then
    # Look for any non-system folder (not starting with . and not index.yml or mappings)
    project_folder=$(find "$TEST_DIR/.tasks" -maxdepth 1 -type d ! -name ".*" ! -name ".tasks" | head -1 | xargs basename 2>/dev/null)
fi

if [ -n "$project_folder" ] && [ -d "$TEST_DIR/.tasks/$project_folder" ]; then
    PASSED=$((PASSED + 1))
    echo -e "${GREEN}✅ Project directory created${NC}"
else
    FAILED=$((FAILED + 1))
    echo -e "${RED}❌ Project directory not created${NC}"
fi
TOTAL=$((TOTAL + 1))

# Test 21: Verify task files exist (look for .yml files in the actual project folder)
task_files=0
if [ -n "$project_folder" ] && [ -d "$TEST_DIR/.tasks/$project_folder" ]; then
    task_files=$(find "$TEST_DIR/.tasks/$project_folder" -name "*.yml" ! -name "metadata.yml" | wc -l)
fi

if [ "$task_files" -gt 0 ]; then
    PASSED=$((PASSED + 1))
    echo -e "${GREEN}✅ Task files created (${task_files// /} files)${NC}"
else
    FAILED=$((FAILED + 1))
    echo -e "${RED}❌ No task files found${NC}"
fi
TOTAL=$((TOTAL + 1))

# Performance tests
echo -e "\n${BLUE}⚡ Performance Tests${NC}"
echo -e "${BLUE}====================${NC}"

# Test 22: Large batch task creation
echo -e "\n${YELLOW}Creating 100 tasks for performance test...${NC}"
for i in {1..100}; do
    cd "$TEST_DIR" && $BINARY_PATH task add --title="Performance Test Task $i" --project=perf-test >/dev/null 2>&1
done

start_time=$(date +%s%N)
run_test "List 100+ Tasks Performance" "$BINARY_PATH task list --project=perf-test" 0 "Listing tasks for project"
end_time=$(date +%s%N)
duration=$(((end_time - start_time) / 1000000))
echo -e "${BLUE}List operation took: ${duration}ms${NC}"

# Final report
echo -e "\n${BLUE}📊 Test Results Summary${NC}"
echo -e "${BLUE}========================${NC}"
echo -e "Total tests: $TOTAL"
echo -e "${GREEN}Passed: $PASSED${NC}"
echo -e "${RED}Failed: $FAILED${NC}"

if [ $FAILED -eq 0 ]; then
    echo -e "\n${GREEN}🎉 All tests passed!${NC}"
    exit 0
else
    echo -e "\n${RED}❌ Some tests failed. Please review the output above.${NC}"
    exit 1
fi
