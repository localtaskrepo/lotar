# Implementation Roadmap

## Phase 1: Core Task Management (High Priority)

### 1.1 Status Management System
**Current State**: Tasks have no status field or workflow management
**Required**:
- Add `status` field to Task struct
- Implement status enum (TODO, IN_PROGRESS, VERIFY, BLOCKED, DONE)
- Create transitions.yaml configuration
- Add status validation logic

**Files to modify**:
- `src/store.rs`: Add status field and enum
- `src/tasks.rs`: Add status update commands
- Add `transitions.yaml` configuration file

### 1.2 Enhanced CLI Commands
**Current State**: Basic `add` and `edit` commands exist
**Required**:
- Implement `status` command for updating task status
- Add `search` command with filtering
- Improve argument parsing (currently very basic)
- Add interactive mode (`-i` flag)

**Files to modify**:
- `src/tasks.rs`: Add new commands and better parsing
- `src/main.rs`: Add new command routing

### 1.3 Index System for Performance
**Current State**: No indexing, linear search only
**Required**:
- Create `index.json` management
- Implement `id2file`, `tag2id`, `file2file` mappings
- Add index updates on task modifications
- Fast lookup functions

**Files to create**:
- `src/index.rs`: Index management module

## Phase 2: Web Interface Completion (Medium Priority)

### 2.1 API Implementation
**Current State**: API routes defined but not implemented
**Required**:
- Complete REST API endpoints in `src/routes.rs`
- Add CRUD operations for tasks
- Add search/filter endpoints
- Proper error handling and JSON responses

### 2.2 React Frontend Functionality
**Current State**: Components exist but lack functionality
**Required**:
- Connect React components to API
- Implement task creation/editing forms
- Add search and filtering UI
- Status management interface

**Files to modify**:
- `view/TasksList.jsx`
- `view/TaskDetails.jsx`
- `view/TaskStatistics.jsx`

## Phase 3: Git Integration (Medium Priority)

### 3.1 Git Awareness
**Current State**: No git integration
**Required**:
- Detect git repository state
- Show task commit status
- Link tasks to branches
- Git hooks for automatic updates

**Files to create**:
- `src/git.rs`: Git integration module

### 3.2 Commit Integration
**Required**:
- Update task status on commits
- Task references in commit messages
- Automatic task state synchronization

## Phase 4: Advanced Features (Low Priority)

### 4.1 TODO Scanner Integration
**Current State**: Scanner exists but isolated
**Required**:
- Link scanned TODOs to task system
- Auto-create tasks from TODO comments
- Update source files with task IDs

### 4.2 IDE Plugin Development
**Current State**: Not started
**Required**:
- IntelliJ plugin for task management
- Integration with existing IDE workflows
- Task creation from IDE

### 4.3 Configuration System
**Current State**: Hardcoded values
**Required**:
- Configurable task fields
- Custom workflows
- Project-specific settings

## Immediate Next Steps

1. **Fix Task Structure** (1-2 days)
   - Add missing fields (status, modified)
   - Update YAML serialization
   - Test with existing tasks

2. **Implement Status Commands** (2-3 days)
   - Add status update CLI command
   - Create transition validation
   - Update web API

3. **Build Index System** (3-4 days)
   - Create index.json management
   - Implement fast lookups
   - Update on task changes

4. **Complete Search Functionality** (2-3 days)
   - CLI search command
   - API search endpoints
   - Web interface search

## Testing Strategy

Each phase should include:
- Unit tests for core functionality
- Integration tests for CLI commands
- End-to-end tests for web interface
- Manual testing with real tasks

## Notes

The QuickJS issues mentioned in README appear to be resolved - the current implementation uses static file serving which is the correct approach. The architecture is sound and most building blocks are in place.
