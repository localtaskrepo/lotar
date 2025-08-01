# CLI Testing Framework

This directory contains a comprehensive testing framework for the LoTaR CLI system, designed to make it easy to test existing functionality and add tests for new commands.

## Test Structure

### 1. Integration Tests (`cli_test_utils.rs`)
- **Purpose**: Test the complete CLI binary with real command-line arguments
- **Use Case**: End-to-end testing, user experience validation
- **Example**: Testing `lotar --experimental add "task"` command

### 2. Unit Tests (`handler_unit_tests.rs`)
- **Purpose**: Test CLI handlers directly without command-line parsing
- **Use Case**: Fast, isolated testing of business logic
- **Example**: Testing `AddHandler::execute()` directly

### 3. End-to-End Tests (`experimental_cli_tests.rs`)
- **Purpose**: Complete workflow testing with realistic scenarios
- **Use Case**: Integration testing, regression testing
- **Example**: Multi-project workflows with validation

### 4. Test Framework (`test_framework.rs`)
- **Purpose**: Test organization, reporting, and utilities
- **Use Case**: Running specific test categories, performance monitoring

## Quick Start

### Running Tests

```bash
# Run all CLI tests
cargo test cli

# Run specific test file
cargo test --test experimental_cli_tests

# Run with output visible
cargo test --test experimental_cli_tests -- --nocapture

# Run a specific test
cargo test test_custom_field_validation_success
```

### Creating New Tests

#### 1. Simple Integration Test
```rust
#[test]
fn test_my_new_command() {
    let harness = TestDataBuilder::basic_environment();
    
    let assert = harness.experimental_cmd()
        .args(["my-command", "arg1", "arg2"])
        .assert();
    
    assert.success()
        .stdout(predicate::str::contains("Expected output"));
}
```

#### 2. Handler Unit Test
```rust
#[test]
fn test_my_handler() {
    let harness = HandlerTestHarness::new();
    harness.setup_configs().expect("Config setup should succeed");
    
    let result = MyHandler::execute(args, None, &harness.resolver);
    assert!(result.is_ok());
}
```

#### 3. Complex Workflow Test
```rust
#[test]
fn test_complex_workflow() {
    let harness = TestDataBuilder::multi_project_environment()
        .expect("Multi-project setup should succeed");
    
    // Step 1: Create tasks
    harness.add_task_to_project("PROJ1", "Task 1").success();
    
    // Step 2: Modify tasks
    harness.change_status("PROJ1-1", "Done").success();
    
    // Step 3: Verify results
    let assert = harness.list_tasks_for_project("PROJ1");
    CliAssertions::assert_task_count(assert, 1);
}
```

## Test Utilities

### CliTestHarness
Main testing utility that provides:
- Isolated temporary directories
- Configuration setup
- Command execution helpers
- File system verification

```rust
let harness = CliTestHarness::new();
harness.setup_test_environment().unwrap();

// Create task with custom fields
let assert = harness.add_task_with_fields(
    "Task title",
    &[("epic", "story-123"), ("sprint", "v1.2")]
);
```

### TestDataBuilder
Pre-configured test environments:

```rust
// Basic environment (global config + one project)
let harness = TestDataBuilder::basic_environment();

// Multi-project environment (global + 3 projects with different configs)
let harness = TestDataBuilder::multi_project_environment().unwrap();
```

### CliAssertions
Common assertion patterns:

```rust
// Assert task was created successfully
CliAssertions::assert_task_created(assert, "PROJECT_NAME");

// Assert validation error occurred
CliAssertions::assert_validation_error(assert, "field_name");

// Assert specific number of tasks listed
CliAssertions::assert_task_count(assert, 3);
```

## Configuration Testing

### Global vs Project Configuration
```rust
// Test with global wildcard config
let harness = CliTestHarness::new();
let global_config = harness.default_global_config();
harness.setup_global_config(global_config).unwrap();

// Test with strict project config
let project_config = harness.strict_project_config("PROJ", vec![
    "epic".to_string(),
    "sprint".to_string()
]);
harness.setup_project_config("PROJ", project_config).unwrap();
```

### Validation Testing
```rust
// Test valid custom field
harness.add_task_to_project_with_fields(
    "PROJ", "Task", &[("epic", "value")]
).success();

// Test invalid custom field
harness.add_task_to_project_with_fields(
    "PROJ", "Task", &[("invalid", "value")]
).failure();
```

## Adding Tests for New Commands

### 1. Create Handler Test
```rust
// In handler_unit_tests.rs
#[test]
fn test_new_command_handler() {
    let harness = HandlerTestHarness::new();
    harness.setup_configs().unwrap();
    
    let result = NewCommandHandler::execute(args, None, &harness.resolver);
    assert!(result.is_ok());
    
    // Verify side effects
    assert!(harness.task_exists("PROJECT", "1"));
}
```

### 2. Create Integration Test
```rust
// In experimental_cli_tests.rs
#[test]
fn test_new_command_integration() {
    let harness = TestDataBuilder::basic_environment();
    
    let assert = harness.experimental_cmd()
        .args(["new-command", "arg1"])
        .assert();
    
    assert.success();
}
```

### 3. Add to Test Framework
```rust
// In test_framework.rs, add to appropriate category
fn run_basic_tests(&self) -> CategoryResult {
    let mut result = CategoryResult::new();
    
    result.add_test("new_command", true, "✅ New command works");
    // ... other tests
    
    result
}
```

## Common Test Patterns

### Error Testing
```rust
// Test validation errors
harness.add_task_to_project_with_fields(
    "PROJ", "Task", &[("invalid_field", "value")]
).failure()
.stderr(predicate::str::contains("validation failed"));

// Test missing required arguments
harness.experimental_cmd()
    .args(["command-missing-args"])
    .assert()
    .failure();
```

### Configuration Testing
```rust
// Test inheritance
let harness = TestDataBuilder::multi_project_environment().unwrap();

// Global allows wildcard, project is strict
harness.add_task_with_fields("Global task", &[("any", "value")]).success();
harness.add_task_to_project_with_fields("STRICT", "Task", &[("any", "value")]).failure();
```

### Performance Testing
```rust
#[test]
fn test_performance() {
    let harness = TestDataBuilder::basic_environment();
    
    let start = std::time::Instant::now();
    
    for i in 1..=1000 {
        harness.add_task(&format!("Task {}", i)).success();
    }
    
    let duration = start.elapsed();
    assert!(duration < std::time::Duration::from_secs(10)); // Should be fast
}
```

## Best Practices

1. **Use appropriate test level**: Unit tests for logic, integration tests for workflows
2. **Clean test environments**: Each test gets a fresh temporary directory
3. **Test both success and failure cases**: Ensure validation and error handling work
4. **Use descriptive test names**: `test_custom_field_validation_with_strict_project`
5. **Test configuration scenarios**: Global vs project configs, inheritance
6. **Verify side effects**: Check that files are created, modified correctly
7. **Test edge cases**: Empty inputs, invalid formats, non-existent resources

## Debugging Tests

### View Test Output
```bash
# See stdout/stderr from tests
cargo test test_name -- --nocapture

# Run single test with full output
cargo test test_custom_field_validation -- --nocapture --show-output
```

### Inspect Test Directories
```rust
// In test, print the temp directory path
println!("Test directory: {:?}", harness.root_path());

// Then manually inspect the .tasks directory structure
```

### Use Debug Assertions
```rust
// Add debug output to understand test state
eprintln!("Current projects: {:?}", harness.get_projects());
eprintln!("Task count in PROJ: {}", harness.count_tasks("PROJ"));
```

## Future Extensions

When adding new CLI commands, extend the testing framework:

1. Add new assertion helpers to `CliAssertions`
2. Add new command helpers to `CliTestHarness`
3. Create handler-specific test utilities in `HandlerTestHarness`
4. Add new test categories to `TestCategory` enum
5. Update `TestDataBuilder` with new environment configurations

This framework is designed to grow with the CLI system and make testing new features straightforward and consistent.
