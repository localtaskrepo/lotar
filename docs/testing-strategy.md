# Testing Strategy for LoTaR

*Last Updated: 2025-07-29*

## Overview

This document outlines the comprehensive testing strategy for LoTaR, combining traditional software testing with real-world dogfooding to ensure the system works reliably for git-native requirements management.

## Testing Philosophy

### Core Principles
1. **Dogfooding First**: Use LoTaR to manage its own development from day one
2. **Git-Native Testing**: Test git integration as thoroughly as the application logic
3. **Multi-Interface Testing**: Ensure CLI, web, IDE, and MCP interfaces work consistently
4. **Real-World Scenarios**: Test with actual project workflows, not just toy examples
5. **Regression Prevention**: Comprehensive test coverage to prevent breaking changes

### Testing Pyramid Approach
```
                    Manual/Exploratory
                   ↗                   ↖
            E2E Tests                    Integration Tests
           ↗                                           ↖
    CLI Tests                                           Web Tests
   ↗                                                           ↖
Unit Tests ←→ Git Tests ←→ File System Tests ←→ MCP Tests ←→ Performance Tests
```

## Testing Levels and Tools

### 1. Unit Testing
**Framework**: Standard Rust testing with `cargo test`
**Coverage**: Individual functions, data structures, parsing logic

```rust
// Example unit test structure
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_task_creation() {
        let task = Task::new("Test Task", "test-project");
        assert_eq!(task.title, "Test Task");
        assert_eq!(task.status, TaskStatus::Todo);
        assert!(task.id.len() > 0);
    }
    
    #[test]
    fn test_yaml_parsing() {
        let yaml = r#"
        ---
        id: "TEST-001"
        title: "Test Task"
        status: "TODO"
        ---
        # Description
        "#;
        
        let task = Task::from_yaml(yaml).unwrap();
        assert_eq!(task.id, "TEST-001");
    }
    
    #[test]
    fn test_external_reference_parsing() {
        let github_ref = ExternalReference::parse("github:org/repo#123").unwrap();
        match github_ref {
            ExternalReference::GitHub { org, repo, issue_number } => {
                assert_eq!(org, Some("org".to_string()));
                assert_eq!(repo, Some("repo".to_string()));
                assert_eq!(issue_number, 123);
            }
            _ => panic!("Expected GitHub reference"),
        }
    }
}
```

**Testing Tools:**
- `cargo test` - Built-in Rust testing
- `tempfile` - Temporary directories for file system tests
- `mockall` - Mocking framework for external dependencies
- `proptest` - Property-based testing for edge cases

### 2. Git Integration Testing
**Framework**: Custom git testing framework
**Coverage**: All git operations, commit generation, history analysis

```rust
// Git-specific testing utilities
pub struct GitTestRepo {
    temp_dir: TempDir,
    repo: git2::Repository,
}

impl GitTestRepo {
    pub fn new() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let repo = git2::Repository::init(temp_dir.path()).unwrap();
        Self { temp_dir, repo }
    }
    
    pub fn create_commit(&self, message: &str, files: &[(&str, &str)]) -> git2::Oid {
        // Helper to create test commits
    }
    
    pub fn assert_commit_message_contains(&self, commit_id: git2::Oid, expected: &str) {
        // Verify commit messages are properly formatted
    }
}

#[test]
fn test_task_update_creates_meaningful_commit() {
    let git_repo = GitTestRepo::new();
    let mut lotar_repo = LoTaRRepository::new(git_repo.path());
    
    let task = Task::new("Test Task", "test-project");
    let task_id = lotar_repo.create_task(task).unwrap();
    
    lotar_repo.update_task(&task_id, |t| {
        t.status = TaskStatus::InProgress;
        t.assignee = Some("john.doe".to_string());
    }).unwrap();
    
    let commit = git_repo.get_last_commit();
    git_repo.assert_commit_message_contains(commit, "TODO → IN_PROGRESS");
    git_repo.assert_commit_message_contains(commit, "Assigned to john.doe");
}
```

### 3. File System Testing
**Framework**: Integration tests with temporary directories
**Coverage**: Task file creation, YAML parsing, file structure validation

```rust
#[test]
fn test_task_file_structure() {
    let temp_dir = TempDir::new().unwrap();
    let repo = LoTaRRepository::new(temp_dir.path());
    
    let task = Task::new("Test Task", "test-project");
    let task_id = repo.create_task(task).unwrap();
    
    // Verify file was created in correct location
    let task_file = temp_dir.path()
        .join(".tasks")
        .join("test-project")
        .join("general")
        .join(format!("{}.md", task_id));
    
    assert!(task_file.exists());
    
    // Verify file format
    let content = std::fs::read_to_string(&task_file).unwrap();
    assert!(content.starts_with("---"));
    assert!(content.contains("id: \"TEST-001\""));
    assert!(content.contains("# Test Task"));
}
```

### 4. CLI Testing
**Framework**: Integration tests with command execution
**Coverage**: All CLI commands, argument parsing, output formatting

```rust
use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn test_cli_task_creation() {
    let temp_dir = TempDir::new().unwrap();
    
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&["task", "create", "--title=Test Task", "--type=feature"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Created task"));
    
    // Verify task file was created
    let tasks_dir = temp_dir.path().join(".tasks");
    assert!(tasks_dir.exists());
}

#[test]
fn test_cli_task_listing() {
    let temp_dir = setup_test_project_with_tasks();
    
    let mut cmd = Command::cargo_bin("lotar").unwrap();
    cmd.current_dir(&temp_dir)
        .args(&["task", "list", "--status=TODO"])
        .assert()
        .success()
        .stdout(predicate::str::contains("AUTH-001"))
        .stdout(predicate::str::contains("TODO"));
}
```

**CLI Testing Tools:**
- `assert_cmd` - Command line testing framework
- `predicates` - Assertions for CLI output
- `tempfile` - Isolated test environments

### 5. Web Interface Testing
**Framework**: Integration tests with headless browser
**Coverage**: React components, API endpoints, user workflows

```rust
// API endpoint testing
#[tokio::test]
async fn test_api_task_creation() {
    let app = create_test_app().await;
    
    let task_data = json!({
        "title": "Test Task",
        "type": "feature",
        "project": "test-project"
    });
    
    let response = app
        .oneshot(
            Request::builder()
                .method(http::Method::POST)
                .uri("/api/tasks")
                .header(http::header::CONTENT_TYPE, mime::APPLICATION_JSON.as_ref())
                .body(Body::from(task_data.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    
    assert_eq!(response.status(), StatusCode::CREATED);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let task: Task = serde_json::from_slice(&body).unwrap();
    assert_eq!(task.title, "Test Task");
}
```

**Frontend Testing:**
```javascript
// Jest + React Testing Library for component tests
import { render, screen, fireEvent } from '@testing-library/react';
import { TaskForm } from './TaskForm';

test('creates task with form submission', async () => {
  const mockOnSubmit = jest.fn();
  render(<TaskForm onSubmit={mockOnSubmit} />);
  
  fireEvent.change(screen.getByLabelText('Title'), {
    target: { value: 'Test Task' }
  });
  fireEvent.click(screen.getByText('Create Task'));
  
  expect(mockOnSubmit).toHaveBeenCalledWith({
    title: 'Test Task',
    type: 'feature'
  });
});
```

**Web Testing Tools:**
- `axum-test` - Axum web framework testing
- `tower-test` - Service testing utilities
- `jest` + `@testing-library/react` - Frontend component testing
- `playwright` - E2E browser testing

### 6. MCP Integration Testing
**Framework**: Mock MCP client and server testing
**Coverage**: All MCP tools, external system integration

```rust
#[tokio::test]
async fn test_mcp_task_creation() {
    let mcp_server = MockMCPServer::new();
    let client = MCPClient::connect_to_mock(mcp_server).await;
    
    let result = client.call_tool("create_task", json!({
        "title": "Test Task via MCP",
        "type": "feature",
        "project": "test-project"
    })).await.unwrap();
    
    assert_eq!(result["task"]["title"], "Test Task via MCP");
    assert!(result["git_commit"].as_str().unwrap().len() > 0);
}

#[test]
fn test_external_reference_validation() {
    let validator = ExternalSystemValidator::new();
    
    let github_ref = "github:org/repo#123";
    let result = validator.validate_reference(github_ref).unwrap();
    
    assert!(result.is_valid);
    assert_eq!(result.system, ExternalSystem::GitHub);
    assert_eq!(result.organization, Some("org".to_string()));
}
```

### 7. Performance Testing
**Framework**: Criterion for benchmarking
**Coverage**: Git operations, file parsing, query performance

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion};

fn benchmark_task_creation(c: &mut Criterion) {
    let temp_dir = TempDir::new().unwrap();
    let mut repo = LoTaRRepository::new(temp_dir.path());
    
    c.bench_function("create_task", |b| {
        b.iter(|| {
            let task = Task::new(
                black_box("Benchmark Task"),
                black_box("benchmark-project")
            );
            repo.create_task(black_box(task))
        })
    });
}

fn benchmark_task_listing(c: &mut Criterion) {
    let repo = setup_repo_with_1000_tasks();
    
    c.bench_function("list_tasks", |b| {
        b.iter(|| {
            repo.list_tasks(black_box(TaskFilter::default()))
        })
    });
}

criterion_group!(benches, benchmark_task_creation, benchmark_task_listing);
criterion_main!(benches);
```

## Dogfooding Strategy

### Phase 1: Self-Hosting Setup
**Goal**: Use LoTaR to manage its own development from the MVP onwards

```bash
# Initialize LoTaR for its own development
cd /Users/mallox/Development/LocalTaskRepoRust
./target/release/lotar init --project="lotar-development"

# Create initial development tasks
lotar task create --title="Implement basic CLI" --type="feature" --epic="phase-1"
lotar task create --title="Add web interface" --type="feature" --epic="phase-2"
lotar task create --title="Create VS Code extension" --type="feature" --epic="phase-3"

# Link to GitHub issues
lotar task link LOTAR-001 --github="#1" --type="implements"
```

### Phase 2: Real-World Usage Patterns
**Test Scenarios:**
1. **Sprint Planning**: Use LoTaR to plan development sprints
2. **Bug Tracking**: Report and track bugs discovered during development
3. **Feature Requests**: Manage feature requests from documentation feedback
4. **External Integration**: Link tasks to GitHub issues for this repository

### Phase 3: Team Collaboration Testing
**Simulate Team Workflows:**
```bash
# Multiple developer scenario
git checkout -b feature/web-interface
lotar task update LOTAR-002 --status="IN_PROGRESS" --assignee="developer-1"

# Code review integration
# (Test in actual PR workflows)

# Merge and status updates
git checkout main
git merge feature/web-interface
lotar task update LOTAR-002 --status="DONE"
```

## Test Data Management

### Test Fixtures
```rust
// Standardized test data
pub struct TestFixtures {
    pub simple_task: Task,
    pub complex_task_with_relationships: Task,
    pub external_linked_task: Task,
}

impl TestFixtures {
    pub fn new() -> Self {
        Self {
            simple_task: Task {
                id: "TEST-001".to_string(),
                title: "Simple Test Task".to_string(),
                status: TaskStatus::Todo,
                // ... other fields
            },
            // ... other fixtures
        }
    }
}
```

### Mock External Systems
```rust
// Mock GitHub API for testing
pub struct MockGitHubAPI {
    issues: HashMap<u32, GitHubIssue>,
}

impl MockGitHubAPI {
    pub fn with_issues(issues: Vec<GitHubIssue>) -> Self {
        let mut map = HashMap::new();
        for issue in issues {
            map.insert(issue.number, issue);
        }
        Self { issues: map }
    }
    
    pub fn get_issue(&self, number: u32) -> Option<&GitHubIssue> {
        self.issues.get(&number)
    }
}
```

## Continuous Integration Strategy

### GitHub Actions Workflow
```yaml
# .github/workflows/test.yml
name: Test Suite

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      
      - name: Install Rust
        uses: actions-rs/toolchain@v1
        with:
          toolchain: stable
          
      - name: Run unit tests
        run: cargo test --lib
        
      - name: Run integration tests
        run: cargo test --test '*'
        
      - name: Run CLI tests
        run: cargo test --test cli_tests
        
      - name: Check code coverage
        run: cargo tarpaulin --out xml
        
      - name: Upload coverage
        uses: codecov/codecov-action@v3
        
  frontend-test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-node@v3
        with:
          node-version: '18'
      
      - name: Install dependencies
        run: npm install
        
      - name: Run frontend tests
        run: npm test
        
      - name: Run E2E tests
        run: npm run test:e2e
```

## Test Coverage Goals

### Coverage Targets
- **Unit Tests**: 90%+ line coverage
- **Integration Tests**: 100% of public API endpoints
- **CLI Tests**: 100% of command combinations
- **Git Operations**: 100% of git integration paths
- **Error Handling**: 100% of error conditions

### Quality Gates
```rust
// Enforce coverage in CI
#[cfg(test)]
mod coverage_tests {
    use tarpaulin;
    
    #[test]
    fn enforce_minimum_coverage() {
        let coverage = tarpaulin::get_coverage_percentage();
        assert!(coverage >= 90.0, "Coverage below 90%: {:.1}%", coverage);
    }
}
```

## Testing Tools and Dependencies

### Cargo.toml Test Dependencies
```toml
[dev-dependencies]
# Core testing
tokio-test = "0.4"
tempfile = "3.0"
assert_cmd = "2.0"
predicates = "3.0"

# Mocking and fixtures
mockall = "0.11"
wiremock = "0.5"

# Property testing
proptest = "1.0"
quickcheck = "1.0"

# Performance testing
criterion = "0.5"

# Git testing
git2 = "0.17"

# Web testing
axum-test = "0.13"
tower-test = "0.4"
hyper = "0.14"

# Coverage
tarpaulin = "0.25"
```

### Frontend Testing Dependencies
```json
{
  "devDependencies": {
    "@testing-library/react": "^13.0.0",
    "@testing-library/jest-dom": "^5.16.0",
    "@testing-library/user-event": "^14.0.0",
    "jest": "^29.0.0",
    "playwright": "^1.35.0",
    "@playwright/test": "^1.35.0"
  }
}
```

## Success Metrics

### Quantitative Metrics
- **Test Coverage**: >90% line coverage
- **Performance**: All operations <100ms in tests
- **Reliability**: 0 test flakes in CI
- **Git Integrity**: 100% of git operations tested

### Qualitative Metrics
- **Dogfooding Success**: LoTaR manages its own development effectively
- **Real-World Validation**: Tasks, relationships, and workflows work as designed
- **Developer Experience**: Testing is fast and reliable for development

### Automated Quality Checks
```bash
# Pre-commit hook script
#!/bin/bash
set -e

echo "Running pre-commit checks..."

# Rust formatting
cargo fmt --check

# Rust linting
cargo clippy -- -D warnings

# Run fast tests
cargo test --lib

# Frontend linting
npm run lint

# Frontend tests
npm test --watchAll=false

echo "All checks passed!"
```

This comprehensive testing strategy ensures LoTaR is robust, reliable, and ready for real-world use while using itself as the primary testing ground for validation.
