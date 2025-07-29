# Architecture Decisions - Git-Native Requirements Management

*Last Updated: 2025-07-28*
*Revision: Updated with MCP integration for AI agents*

## Overview

This document captures the **bold architectural decisions** for LoTaR as the first **git-native requirements management system**. Every decision serves the primary goal: creating immutable decision audit trails.

## AD-001: Git as the Single Source of Truth

### Decision
**Git IS the database** - no external storage, no separate audit logs, no secondary systems.

### Rationale
- **Immutable History**: Git's cryptographic hashing ensures tamper-proof audit trails
- **Distributed Collaboration**: Every clone has complete project history
- **Proven Merge Algorithms**: 15+ years of conflict resolution refinement
- **Compliance Ready**: Cryptographic signatures satisfy SOX/FDA requirements
- **Developer Native**: Zero learning curve for git-proficient teams

### Implementation Details
```rust
// No database connections, no external dependencies
pub struct LoTaRRepository {
    git_repo: git2::Repository,
    tasks_path: PathBuf,        // .tasks/ directory
    current_branch: String,
}

// All operations go through git
impl LoTaRRepository {
    pub fn update_task(&mut self, task: &Task) -> Result<CommitId> {
        self.write_task_file(task)?;
        self.git_add_task_file(task)?;
        self.git_commit_with_context(task)
    }
}
```

### Alternatives Rejected
- **SQLite + Git**: Dual storage adds complexity and sync issues
- **JSON in Git**: Binary formats don't diff/merge well
- **External Database**: Breaks distributed model and audit trail integrity

### Implications
- Every task change creates git commit
- Complete audit trail is automatic
- No database administration required
- Performance depends on git repository size
- Conflict resolution uses git merge tools

## AD-002: Human-First File Format for Git Optimization

### Decision
**Markdown files with YAML frontmatter** - optimized for human readability and git diff clarity.

### File Structure Design
```yaml
---
# YAML frontmatter: structured metadata
id: "AUTH-001"
title: "Implement OAuth Authentication"
status: "IN_PROGRESS"
---

# Markdown body: rich content for humans
## Context and Background
[Rich formatted content...]
```

### Git Diff Optimization
```diff
# Beautiful, meaningful diffs
+oauth_providers: ["google", "github"]  # Was just ["google"]
+decision_context: "Customer integration request"
```

### Rationale
- **Human Readable**: Developers can read raw files without tools
- **Git Friendly**: YAML produces clean, meaningful diffs
- **Rich Content**: Markdown supports formatting, links, code blocks
- **Tool Compatible**: Standard formats work with existing tools
- **Merge Friendly**: Structured format enables intelligent merging

### Alternatives Rejected
- **Pure JSON**: Not human-readable, poor git diffs
- **Custom Binary**: Vendor lock-in, no tool compatibility
- **XML**: Verbose, poor readability
- **Database Records**: Not git-friendly, no offline access

## AD-003: Primary Interface Strategy

### Decision
**Web interface as primary daily-use tool, IDE plugins for code integration, CLI for essential operations** - focus development effort where users spend most time.

### Interface Hierarchy
```rust
// Primary interfaces by use case
pub enum InterfaceType {
    WebInterface,    // Primary: Daily task management, planning, reporting
    IDEPlugin,       // Secondary: Code-task integration, contextual info
    CLI,             // Supporting: Automation, git operations, essential CRUD
    MCPServer,       // AI Interface: Agent-optimized operations
}
```

### Web Interface (Primary)
- **Daily task management**: Create, edit, update tasks with rich UI
- **Project management**: Burndown charts, velocity tracking, timeline views
- **Team collaboration**: Assignment, workload balancing, status dashboards
- **Filtering and search**: Advanced filtering with auto-generated components
- **Git history visualization**: Show decision trails from git commits

### IDE Plugins (Code Integration)
- **Task context**: Show related tasks without leaving editor
- **TODO linking**: Connect code comments to formal tasks
- **Quick operations**: Status updates, commenting from editor
- **Branch integration**: Tasks associated with current git branch

### CLI (Essential Operations)
- **Core CRUD**: Create, update, list, show tasks
- **Git integration**: Commit generation, history analysis
- **Automation**: Scripting, CI/CD integration
- **Configuration**: Schema setup, field definitions

### Rationale
- **User Focus**: Most time spent in web interface for management tasks
- **Context Switching**: IDE plugins eliminate switching for code-related tasks
- **Automation**: CLI enables scripting and integration workflows
- **Development Efficiency**: Focus UI effort where it provides most value

### Implications
- Web interface gets majority of development attention
- CLI focused on essential operations, not comprehensive UI
- IDE plugins provide seamless integration without duplication
- Consistent data model across all interfaces

## AD-004: Pragmatic Field System Design

### Decision
**Built-in standard fields for project management + configurable custom fields with generic UI treatment** - balance standardization where it matters with flexibility where teams differ.

### Field Architecture
```rust
// Built-in fields with special handling
pub struct Task {
    // Core identity (always required)
    pub id: String,
    pub title: String,
    pub status: TaskStatus,                    // Built-in enum with extensions
    
    // Standard project management fields
    pub priority: Priority,                    // Built-in enum: LOW, MEDIUM, HIGH, CRITICAL
    pub task_type: TaskType,                   // Built-in enum: feature, bug, epic, spike, chore
    pub assignee: Option<String>,              // Autocomplete from git history
    pub project: String,                       // String with autocomplete
    pub created: DateTime<Utc>,                // Auto-generated
    pub modified: DateTime<Utc>,               // Auto-updated
    pub due_date: Option<NaiveDate>,          // Date picker in UI
    pub effort: Option<Effort>,               // Special effort field with unit conversion
    
    // Built-in structured fields
    pub acceptance_criteria: Vec<String>,      // Multi-line editor
    pub relationships: TaskRelationships,      // Special relationship UI
    pub comments: Vec<Comment>,                // Special commenting interface
    
    // Custom fields (generic handling)
    pub custom_fields: HashMap<String, CustomFieldValue>,
}
```

### Rationale
- **Project Management Features**: Built-in fields enable proper burndown charts, velocity tracking, timeline views
- **Team Flexibility**: Custom fields adapt to team-specific needs (regulatory, business value, etc.)
- **Automatic UI**: Custom fields get appropriate components without manual UI development
- **Data Integrity**: Built-in fields have validation, custom fields have type safety
- **Git Optimization**: Both field types produce clean, meaningful diffs

## AD-005: Zero External Dependencies for Core Functions

### Decision
**Core functionality works offline with zero external services** - only git and filesystem required.

### Dependency Strategy
```toml
# Cargo.toml - Minimal dependencies for core functionality
[dependencies]
# Core functionality (always included)
git2 = "0.18"           # Git operations
serde = "1.0"           # YAML/JSON serialization  
chrono = "0.4"          # Date/time handling
clap = "4.0"            # CLI interface
thiserror = "1.0"       # Error handling

# Optional features
[dependencies.tokio]
version = "1.0"
optional = true

[dependencies.warp]
version = "0.3"
optional = true

[features]
default = ["web-interface", "mcp-server"]
web-interface = ["tokio", "warp"]
mcp-server = ["tokio", "serde_json"]
```

### Self-Contained Operation
- **No Database**: Git repository IS the database
- **No Cloud Services**: Everything works offline
- **No Network Required**: Full functionality without internet
- **No External APIs**: Self-contained decision audit trails

### Rationale
- **Reliability**: No external service failures
- **Security**: No data leaves local environment
- **Compliance**: Data sovereignty for regulated industries
- **Performance**: No network latency for core operations
- **Simplicity**: Single binary deployment

## AD-006: Compliance-First Design

### Decision
**Audit trail and compliance features are built-in from day one** - not added later.

### Compliance Architecture
```rust
pub struct AuditTrail {
    task_id: String,
    complete_history: Vec<GitCommit>,
    field_changes: Vec<FieldChange>,
    decision_context: Vec<DecisionContext>,
    compliance_markers: ComplianceData,
}

impl AuditTrail {
    pub fn generate_sox_report(&self) -> SoxComplianceReport {
        // Analyzes git history for SOX compliance
        // - All changes with timestamps and attribution
        // - Decision makers and decision context
        // - Change impact analysis
    }
    
    pub fn generate_fda_validation(&self) -> FdaValidationReport {
        // FDA 21 CFR Part 11 compliance
        // - Electronic signatures via git commits
        // - Complete audit trail
        // - Change control documentation
    }
}
```

### Regulatory Support
- **SOX Compliance**: Complete audit trail of business decisions
- **FDA 21 CFR Part 11**: Electronic records and signatures
- **ISO 27001**: Information security management
- **Government Contracts**: Decision accountability requirements

### Rationale
- **Enterprise Ready**: Compliance features drive enterprise adoption
- **Competitive Advantage**: No other task management tool provides this
- **Future Proof**: Regulatory requirements only increase over time
- **Built-In**: Easier to design in than retrofit later

## AD-007: MCP Integration for AI Agent Accessibility

### Decision
**First-class MCP (Model Context Protocol) support** - AI agents get specialized tools and interfaces optimized for their workflows, making LoTaR the first AI-native project management system.

### MCP Architecture Design
```rust
// Dual interface approach: human-optimized and AI-optimized
pub struct LoTaRSystem {
    // Human interfaces
    web_interface: WebServer,
    cli_interface: CLIHandler,
    ide_plugins: IDEIntegrationManager,
    
    // AI interface
    mcp_server: LoTaRMCPServer,
}

pub struct LoTaRMCPServer {
    repository: Arc<RwLock<LoTaRRepository>>,
    nlp_engine: NaturalLanguageProcessor,     // Parse natural language descriptions
    analysis_engine: ProjectAnalysisEngine,   // High-level project insights
    git_analyzer: GitHistoryAnalyzer,        // Decision pattern analysis
    similarity_engine: TaskSimilarityEngine, // Find related tasks
}
```

### AI-Optimized Tool Categories
```rust
// MCP tools organized by AI use cases
pub enum MCPToolCategory {
    // Basic operations (optimized for AI efficiency)
    TaskManagement {
        create_task,
        update_task,
        bulk_update_tasks,
        list_tasks_with_context,
    },
    
    // Natural language processing
    NaturalLanguageOps {
        create_task_from_description,
        parse_requirements_text,
        extract_acceptance_criteria,
        suggest_task_improvements,
    },
    
    // Project analysis and insights
    ProjectAnalysis {
        analyze_project_risks,
        get_critical_path,
        predict_delivery_timeline,
        identify_resource_conflicts,
    },
    
    // Decision and pattern analysis
    DecisionAnalysis {
        analyze_decision_patterns,
        trace_requirement_evolution,
        identify_decision_bottlenecks,
        suggest_process_improvements,
    },
}
```

### Rationale
- **AI Agent Efficiency**: Higher-level operations reduce API calls and improve agent performance
- **Context Awareness**: AI gets project context and historical patterns automatically
- **Natural Language Support**: AI can work with unstructured descriptions naturally
- **Pattern Recognition**: Built-in analysis tools provide insights humans might miss
- **Batch Operations**: Efficient bulk operations for AI workflows
- **Decision Intelligence**: Git history analysis provides decision context

### Alternatives Rejected
- **Generic REST API Only**: Would require AI agents to make many low-level calls
- **File-Only Access**: AI agents would need to parse files manually, losing efficiency
- **External AI Service**: Would break the zero-dependency principle
- **Separate AI Database**: Would duplicate data and break git-native approach

### Implications
- MCP server runs alongside other interfaces
- AI agents get specialized, efficient operations
- Natural language processing enables intuitive task creation
- Project analysis tools provide strategic insights
- Maintains all git-native benefits while optimizing for AI workflows
- Creates feedback loop where AI usage improves the system for humans

### Enterprise AI Benefits
- **AI Project Managers**: Agents can analyze risks, optimize resources, predict timelines
- **Requirements Engineers**: AI can improve acceptance criteria and find gaps
- **Development Coaches**: AI can analyze team patterns and suggest improvements
- **Compliance Assistants**: AI can ensure regulatory requirements are met

## Decision Impact Matrix (Updated)

| Decision | Implementation Complexity | Performance Impact | Compliance Value | Developer Experience | AI Agent Value |
|----------|--------------------------|-------------------|------------------|---------------------|----------------|
| AD-001: Git as Database | Medium | High+ | Excellent | Excellent | Excellent |
| AD-002: Human-First Format | Low | Medium | Good | Excellent | Good |
| AD-003: Primary Interface Strategy | Medium | High+ | Good | Excellent | Good |
| AD-004: Pragmatic Field System | High | Medium | Good | Excellent | Excellent |
| AD-005: Zero Dependencies | Medium | High+ | Excellent | Good | Good |
| AD-006: Compliance-First | High | Medium | Excellent | Medium | High |
| AD-007: MCP Integration | High | Medium | Good | Good | Excellent |

## Revolutionary Technical Outcome (Updated)

These architectural decisions create a **genuinely unique system**:

1. **First git-native requirements management** - no other tool does this
2. **Pragmatic field system** - balances standardization with flexibility
3. **Primary interface focus** - development effort where users spend time
4. **AI-native design** - first project management tool designed for AI agents
5. **Immutable decision audit trails** - perfect for compliance
6. **Distributed collaboration** - works like git, feels like git
7. **Zero vendor lock-in** - standard formats, open source
8. **Enterprise ready** - compliance and audit from day one

This architecture positions LoTaR as **revolutionary requirements engineering** with practical project management capabilities that both humans and AI agents can use effectively.
