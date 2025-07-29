# Feature Specifications - Git-Native Requirements Management

*Last Updated: 2025-07-28*
*Revision: Updated for pragmatic task format with standard + custom fields*

## Revolutionary Feature Set Overview

LoTaR builds **git-native requirements engineering capabilities** with a pragmatic balance of standardized project management fields and flexible custom fields.

## FS-001: Pragmatic Task File Format (Phase 1)
*Standardization where it matters, flexibility where teams differ*

### Vision Statement
Task files that balance **standardized project management** with **team-specific flexibility**, producing beautiful git diffs and enabling rich UI components.

### File Format Specification
```yaml
# Built-in standard fields (special handling in UI)
id: "AUTH-001"
title: "Implement OAuth Authentication"  
status: "TODO"                          # Built-in enum with extensions
priority: "HIGH"                        # Built-in enum: LOW, MEDIUM, HIGH, CRITICAL
type: "feature"                         # Built-in enum: feature, bug, epic, spike, chore
assignee: "john.doe@company.com"        # Autocomplete from git history
project: "webapp"                       # String with autocomplete
created: "2025-07-28T10:00:00Z"         # Auto-generated timestamp
modified: "2025-07-28T14:30:00Z"        # Auto-updated timestamp
due_date: "2025-08-15"                  # Date field with date picker
effort: "5d"                            # Special effort field with unit conversion

# Built-in structured fields (special UI components)
acceptance_criteria:
  - "User can login with Google OAuth"
  - "User can login with GitHub OAuth"
  - "Session expires after 24 hours"
  - "Failed login attempts are logged"

relationships:
  depends_on: ["AUTH-002", "SEC-001"]    # Task dependencies
  blocks: ["USER-005"]                   # Tasks blocked by this one
  related: ["AUTH-003"]                  # General relationships
  parent: ["EPIC-USER-AUTH"]             # Parent epic/story
  child: []                              # Subtasks
  fixes: []                              # Bug fixes
  duplicate_of: []                       # Duplicate tracking

# Team-specific custom fields (generic UI treatment based on type)
epic: "user-management"                 # String → text search + autocomplete
story_points: 8                         # Number → range filter
security_review_required: true          # Boolean → checkbox filter
affected_components: ["frontend", "backend"]  # Array → multi-select
regulatory_requirement: "GDPR"          # String → text search
business_value: "revenue"               # String → dropdown if enum configured

# Comments section (discussion, not git history)
comments:
  - author: "john.doe"
    date: "2025-07-28T10:00:00Z"
    text: "Starting with Google OAuth first, then GitHub"
  - author: "jane.smith"
    date: "2025-07-28T11:00:00Z"
    text: "Security team recommends PKCE flow for mobile clients"
```

# OAuth Authentication Implementation

## Context and Background
Our users currently create separate accounts for our application, creating friction in the signup process.

## Technical Approach
We'll implement OAuth 2.0 using the `oauth2` Rust crate with custom provider configurations.

## Implementation Progress

### 2025-07-28 - Initial Planning
- Researched OAuth 2.0 best practices
- Selected `oauth2` crate for implementation
- Planned Google OAuth integration first

## Files and References
- `src/auth/oauth.rs` - Core OAuth implementation
- `src/auth/providers/` - Provider-specific code

### Built-in Field Benefits
- **effort**: Powers burndown charts, velocity tracking, timeline estimation
- **due_date**: Enables proper timeline views and deadline tracking
- **assignee**: Supports workload balancing and team management
- **priority/status**: Drives proper project management workflows
- **relationships**: Enables dependency tracking and critical path analysis

### Custom Field Flexibility
Teams can add fields like:
- `customer_impact: "high"` (string with text search)
- `story_points: 8` (number with range filters)
- `security_review_required: true` (boolean with checkbox)
- `affected_components: ["frontend", "backend"]` (array with multi-select)

### CLI Interface for Task Files
```bash
# Create task with built-in fields
lotar task create --title="OAuth Implementation" --type="feature" \
  --assignee="john.doe" --priority="HIGH" --effort="5d"

# Update built-in fields
lotar task update AUTH-001 --status="IN_PROGRESS" --due-date="2025-08-15"

# Set custom fields (generic handling)
lotar task update AUTH-001 --set="story_points=8" --set="epic=user-management"

# Add relationships
lotar task relate AUTH-001 --depends-on="AUTH-002" --blocks="USER-005"
```

### Acceptance Criteria
- ✅ Built-in fields enable proper project management features
- ✅ Custom fields get appropriate UI treatment based on type
- ✅ Files produce clean, meaningful git diffs
- ✅ YAML frontmatter validates against schema
- ✅ Effort field supports multiple units ("5d", "40h", "8sp")
- ✅ Relationships are typed and validated

## FS-002: Web Interface Primary Experience (Phase 2)
*The main interface most users will use daily*

### Vision Statement
**Rich web interface** that serves as the primary tool for task management, leveraging built-in fields for project management features and providing automatic UI for custom fields.

### Built-in Field Components
```javascript
// Specialized components for built-in fields
const TaskForm = () => (
  <form>
    <TextInput field="title" required maxLength={200} />
    <StatusDropdown 
      field="status" 
      values={["TODO", "IN_PROGRESS", "REVIEW", "DONE", "CANCELLED"]}
      colorCoded={true}
    />
    <PriorityDropdown 
      field="priority" 
      values={["LOW", "MEDIUM", "HIGH", "CRITICAL"]}
      colorCoded={true}
    />
    <DatePicker 
      field="due_date" 
      showTimeline={true}
      highlightOverdue={true}
    />
    <EffortInput 
      field="effort" 
      supportedUnits={["d", "h", "sp", "w"]}
      convertToDisplay={true}
    />
    <UserSelect 
      field="assignee" 
      autocompleteFromGit={true}
      showWorkload={true}
    />
    <AcceptanceCriteriaEditor 
      field="acceptance_criteria"
      allowReordering={true}
      checklistView={true}
    />
    <RelationshipEditor 
      field="relationships"
      taskSearch={true}
      dependencyValidation={true}
    />
  </form>
);
```

### Custom Field Components (Auto-generated)
```javascript
// Generic components based on field type
const CustomFieldRenderer = ({ fieldName, fieldConfig, value, onChange }) => {
  switch(fieldConfig.type) {
    case 'string':
      return (
        <TextInput 
          value={value}
          onChange={onChange}
          autocomplete={fieldConfig.autocomplete}
          searchable={fieldConfig.searchable}
        />
      );
    case 'number':
      return (
        <NumberInput 
          value={value}
          onChange={onChange}
          min={fieldConfig.min}
          max={fieldConfig.max}
        />
      );
    case 'boolean':
      return <Checkbox checked={value} onChange={onChange} />;
    case 'enum':
      return (
        <Select 
          options={fieldConfig.values}
          value={value}
          onChange={onChange}
        />
      );
    case 'array[string]':
      return (
        <MultiSelect 
          values={value || []}
          onChange={onChange}
          allowCustom={fieldConfig.allowCustom}
        />
      );
    default:
      return <TextInput value={value} onChange={onChange} />;
  }
};
```

### Project Management Features
```javascript
// Built-in fields enable rich project management
const BurndownChart = ({ tasks, sprint }) => {
  // Uses effort field to calculate burndown
  const effortData = tasks.map(task => ({
    date: task.modified,
    remainingEffort: calculateEffort(task.effort, task.status)
  }));
  
  return <LineChart data={effortData} />;
};

const VelocityChart = ({ sprints }) => {
  // Uses story_points custom field if available, falls back to effort
  const velocityData = sprints.map(sprint => ({
    sprint: sprint.name,
    completed: sprint.tasks
      .filter(t => t.status === 'DONE')
      .reduce((sum, t) => sum + (t.story_points || parseEffort(t.effort)), 0)
  }));
  
  return <BarChart data={velocityData} />;
};
```

### Filtering and Search
```javascript
// Auto-generated filters based on field types
const TaskFilters = ({ customFields }) => (
  <div className="filters">
    {/* Built-in field filters (specialized) */}
    <StatusFilter allowMultiple={true} />
    <PriorityFilter allowMultiple={true} />
    <AssigneeFilter showWorkload={true} />
    <DateRangeFilter field="due_date" presets={["overdue", "this_week"]} />
    <EffortRangeFilter normalizeUnits={true} />
    
    {/* Custom field filters (auto-generated) */}
    {customFields.map(field => (
      <CustomFieldFilter 
        key={field.name}
        field={field}
        type={field.type}
      />
    ))}
    
    {/* Text search across all fields */}
    <TextSearch 
      searchFields={["title", "acceptance_criteria", "comments", ...customStringFields]}
    />
  </div>
);
```

### Acceptance Criteria
- ✅ Built-in fields get specialized, rich UI components
- ✅ Custom fields get appropriate UI based on type automatically
- ✅ Project management features work with standardized fields
- ✅ Filtering and search work across all field types
- ✅ Bulk operations preserve field types and validation
- ✅ Real-time updates reflect changes across team

## FS-003: IDE Integration (Phase 3)
*Seamless code-task integration without context switching*

### Vision Statement
**Never leave your editor** - task information and management integrated directly into development environment with rich context about related code.

### VS Code Extension Features
```typescript
// Task panel in sidebar
export class TaskTreeProvider implements TreeDataProvider<TaskItem> {
  getChildren(element?: TaskItem): TaskItem[] {
    if (!element) {
      // Show tasks filtered by current branch, assignee, etc.
      return this.getMyCurrentTasks();
    }
    return [];
  }
  
  getTreeItem(element: TaskItem): TreeItem {
    const item = new TreeItem(element.title);
    item.description = `${element.status} | ${element.priority} | ${element.effort}`;
    item.contextValue = 'task';
    item.command = {
      command: 'lotar.openTask',
      title: 'Open Task',
      arguments: [element.id]
    };
    return item;
  }
}

// Hover provider for task information
export class TaskHoverProvider implements HoverProvider {
  provideHover(document: TextDocument, position: Position): Hover | undefined {
    const taskRef = this.extractTaskReference(document, position);
    if (taskRef) {
      const task = this.getTaskById(taskRef);
      return new Hover([
        `**${task.title}**`,
        `Status: ${task.status} | Priority: ${task.priority}`,
        `Assignee: ${task.assignee} | Due: ${task.due_date}`,
        `Effort: ${task.effort}`,
        '',
        task.acceptance_criteria.map(c => `• ${c}`).join('\n')
      ]);
    }
  }
}
```

### IntelliJ Plugin Features
```kotlin
// Task tool window
class TaskToolWindow(project: Project) : SimpleToolWindowPanel(true, true) {
    private val taskTree = TaskTree(project)
    
    init {
        setContent(ScrollPaneFactory.createScrollPane(taskTree))
        setupToolbar()
    }
    
    private fun setupToolbar() {
        val actionGroup = DefaultActionGroup()
        actionGroup.add(RefreshTasksAction())
        actionGroup.add(CreateTaskAction())
        actionGroup.add(FilterTasksAction())
        
        val toolbar = ActionManager.getInstance()
            .createActionToolbar("TaskToolWindow", actionGroup, true)
        setToolbar(toolbar.component)
    }
}

// Code inspection for TODO linking
class TodoTaskLinkingInspection : LocalInspectionTool() {
    override fun buildVisitor(holder: ProblemsHolder, isOnTheFly: Boolean): PsiElementVisitor {
        return object : PsiElementVisitor() {
            override fun visitComment(comment: PsiComment) {
                val todoText = extractTodoText(comment)
                if (todoText != null && !hasLinkedTask(todoText)) {
                    holder.registerProblem(
                        comment,
                        "TODO comment not linked to task",
                        CreateTaskFromTodoFix(todoText)
                    )
                }
            }
        }
    }
}
```

### Language Server Protocol Implementation
```rust
// LSP server for editor-agnostic integration
impl LanguageServer for LoTaRLanguageServer {
    fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let task_ref = self.extract_task_reference(&params)?;
        if let Some(task_id) = task_ref {
            let task = self.repository.get_task(&task_id)?;
            let hover_content = format!(
                "**{}**\n\nStatus: {} | Priority: {}\nAssignee: {} | Due: {}\nEffort: {}\n\n{}",
                task.title,
                task.status,
                task.priority, 
                task.assignee,
                task.due_date.unwrap_or_default(),
                task.effort,
                task.acceptance_criteria.join("\n• ")
            );
            
            Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: hover_content,
                }),
                range: None,
            }))
        } else {
            Ok(None)
        }
    }
    
    fn code_action(&self, params: CodeActionParams) -> Result<Vec<CodeAction>> {
        let mut actions = Vec::new();
        
        // Check for TODO comments that could become tasks
        if let Some(todo_text) = self.extract_todo_at_position(&params)? {
            actions.push(CodeAction {
                title: "Create task from TODO".to_string(),
                kind: Some(CodeActionKind::QUICKFIX),
                edit: Some(self.create_task_from_todo_edit(todo_text)?),
                ..Default::default()
            });
        }
        
        Ok(actions)
    }
}
```

### Acceptance Criteria
- ✅ Task information available without leaving editor
- ✅ TODO comments link to formal tasks automatically
- ✅ Task creation from code context is seamless
- ✅ Current branch tasks highlighted in task panel
- ✅ Task status updates reflect immediately in editor
- ✅ Git integration shows task-branch associations

## FS-004: Git History Integration (Phase 2)
*Decision history comes from git, not embedded data*

### Vision Statement
**Git history IS the decision trail** - complete audit trail of requirement changes through git commits, with web interface visualization.

### Git History Analysis
```rust
// Analyze git history for task changes
pub struct TaskHistoryAnalyzer {
    repo: git2::Repository,
}

impl TaskHistoryAnalyzer {
    pub fn get_task_history(&self, task_id: &str) -> Result<Vec<TaskHistoryEntry>> {
        let task_file_path = format!(".tasks/{}.md", task_id);
        let mut revwalk = self.repo.revwalk()?;
        revwalk.push_head()?;
        
        let mut history = Vec::new();
        for commit_oid in revwalk {
            let commit = self.repo.find_commit(commit_oid?)?;
            if let Some(changes) = self.analyze_commit_for_task(&commit, &task_file_path)? {
                history.push(TaskHistoryEntry {
                    commit_sha: commit.id().to_string(),
                    author: commit.author().name().unwrap_or("Unknown").to_string(),
                    date: commit.time().seconds(),
                    message: commit.message().unwrap_or("").to_string(),
                    field_changes: changes,
                });
            }
        }
        
        Ok(history)
    }
    
    fn analyze_commit_for_task(&self, commit: &Commit, file_path: &str) -> Result<Option<Vec<FieldChange>>> {
        // Parse diff to extract field changes
        let tree = commit.tree()?;
        let parent_tree = commit.parent(0)?.tree()?;
        
        let diff = self.repo.diff_tree_to_tree(Some(&parent_tree), Some(&tree), None)?;
        
        // Extract changes to YAML frontmatter
        self.parse_yaml_diff_for_changes(&diff, file_path)
    }
}
```

### Web Interface History Display
```javascript
// Git history visualization in web interface
const TaskHistory = ({ taskId }) => {
  const [history, setHistory] = useState([]);
  const [loading, setLoading] = useState(true);
  
  useEffect(() => {
    fetch(`/api/tasks/${taskId}/history`)
      .then(res => res.json())
      .then(data => {
        setHistory(data);
        setLoading(false);
      });
  }, [taskId]);

  if (loading) return <Spinner />;
  
  return (
    <div className="task-history">
      <h3>Change History</h3>
      {history.map(entry => (
        <div key={entry.commit_sha} className="history-entry">
          <div className="commit-header">
            <span className="commit-message">{entry.message}</span>
            <span className="commit-meta">
              {entry.author} - {formatDate(entry.date)}
            </span>
          </div>
          
          <div className="field-changes">
            {entry.field_changes.map(change => (
              <div key={change.field} className="field-change">
                <strong>{change.field}:</strong>
                <span className="old-value">{change.old_value}</span>
                <span className="arrow">→</span>
                <span className="new-value">{change.new_value}</span>
              </div>
            ))}
          </div>
          
          <div className="commit-actions">
            <button onClick={() => showFullDiff(entry.commit_sha)}>
              View Full Diff
            </button>
            <button onClick={() => showBlame(taskId, entry.commit_sha)}>
              Show Blame
            </button>
          </div>
        </div>
      ))}
    </div>
  );
};
```

### Decision Accountability
```bash
# CLI commands for decision tracking
lotar git log AUTH-001                    # Show git history for task
lotar git blame AUTH-001 --field=due_date # Show who changed due date and when
lotar git diff AUTH-001 --from=abc123     # Show changes since specific commit

# Decision analysis
lotar analyze decisions --person="john.doe" --impact="timeline"
lotar analyze patterns --field="priority" --date-range="2025-Q3"
```

### Acceptance Criteria
- ✅ Complete change history derived from git commits
- ✅ Field-level change tracking and visualization
- ✅ Decision accountability through git blame
- ✅ Web interface shows rich history without embedded data
- ✅ CLI provides git-based analysis commands
- ✅ History analysis works with both built-in and custom fields

## FS-005: Simplified MCP Integration for AI Agents (Phase 4)
*Clean MCP interface without complex NLP features*

### Vision Statement
**AI agents as efficient collaborators** - LoTaR provides a streamlined MCP interface that leverages AI agents' existing language capabilities rather than building redundant NLP features.

### Core MCP Tools

#### Task Management Tools
```typescript
interface TaskManagementTools {
  // Basic CRUD operations
  create_task(params: {
    title: string;
    description?: string;
    type?: TaskType;
    priority?: Priority;
    assignee?: string;
    project?: string;
    effort?: string;
    due_date?: string;
    acceptance_criteria?: string[];
    custom_fields?: Record<string, any>;
  }): Promise<{
    task: Task;
    git_commit: string;
  }>;

  update_task(params: {
    id: string;
    updates: Partial<Task>;
  }): Promise<{
    task: Task;
    changes: FieldChange[];
    git_commit: string;
  }>;

  // Batch operations for efficiency
  bulk_create_tasks(params: {
    tasks: CreateTaskParams[];
  }): Promise<{
    created_tasks: Task[];
    git_commit: string;
  }>;

  bulk_update_tasks(params: {
    updates: Array<{
      id: string;
      updates: Partial<Task>;
    }>;
  }): Promise<{
    updated_tasks: Task[];
    git_commit: string;
  }>;
}
```

#### External System Integration Tools
```typescript
interface ExternalSystemTools {
  // Link to external tickets
  link_external_ticket(params: {
    task_id: string;
    external_ref: string;  // "github:org/repo#123", "jira:PROJ-456"
    relationship_type: "implements" | "depends_on" | "blocks" | "references";
  }): Promise<{
    updated_task: Task;
    parsed_reference: ExternalTicketInfo;
  }>;

  // Get external links
  get_external_links(params: {
    project?: string;
    system?: "github" | "jira" | "linear" | "azure" | "asana";
    task_id?: string;
  }): Promise<{
    external_links: ExternalLink[];
  }>;

  // Validate external reference
  validate_external_ref(params: {
    external_ref: string;
  }): Promise<{
    is_valid: boolean;
    parsed_info: ExternalTicketInfo;
    validation_error?: string;
  }>;
}
```

#### Git Context Tools
```typescript
interface GitContextTools {
  // Task history from git
  get_task_history(params: {
    task_id: string;
    include_diffs?: boolean;
    max_entries?: number;
  }): Promise<{
    history: GitHistoryEntry[];
    decision_timeline: DecisionEvent[];
  }>;

  // Find similar tasks
  find_similar_tasks(params: {
    title?: string;
    description?: string;
    task_id?: string;
    project?: string;
    limit?: number;
  }): Promise<{
    similar_tasks: Array<{
      task: Task;
      similarity_score: number;
      matching_fields: string[];
    }>;
  }>;

  // Project context
  get_project_context(params: {
    project: string;
    include_stats?: boolean;
  }): Promise<{
    project_info: ProjectInfo;
    task_counts: TaskCounts;
    recent_activity: ActivityEvent[];
    team_members: string[];
  }>;
}
```

### Enhanced Relationships with External Systems

#### Task File Format Enhancement
```yaml
---
# Standard task fields
id: "AUTH-001"
title: "Implement OAuth Authentication"
# ... other fields ...

# Enhanced relationships
relationships:
  # Internal task relationships
  depends_on: ["AUTH-002", "SEC-001"]
  blocks: ["USER-005"]
  related: ["AUTH-003"]
  
  # External system relationships
  external_links:
    implements: ["github:myorg/frontend#456", "jira:PROJ-789"]
    depends_on: ["github:#123"]  # Current repo context
    references: ["linear:LIN-456", "azure:12345"]
    blocks: ["jira:PROJ-234"]
---
```

#### External Reference Parsing
```rust
// External reference parsing in Rust
pub enum ExternalReference {
    GitHub {
        org: Option<String>,
        repo: Option<String>,
        issue_number: u32,
    },
    Jira {
        project: String,
        issue_number: u32,
    },
    Linear {
        issue_id: String,
    },
    Azure {
        work_item_id: u32,
    },
    Asana {
        task_id: u32,
    },
}

impl ExternalReference {
    pub fn parse(reference: &str) -> Result<Self> {
        if reference.starts_with("github:") {
            Self::parse_github_ref(reference)
        } else if reference.starts_with("jira:") {
            Self::parse_jira_ref(reference)
        } else if reference.starts_with("linear:") {
            Self::parse_linear_ref(reference)
        } else {
            Err(LoTaRError::InvalidExternalRef(reference.to_string()))
        }
    }
}
```

### AI Agent Use Cases (Simplified)

#### Use Case 1: Project Setup Agent
```typescript
class ProjectSetupAgent {
  async createProjectFromSpec(spec: ProjectSpec): Promise<ProjectResult> {
    // AI agent creates multiple related tasks
    const tasks = spec.features.map(feature => ({
      title: feature.title,
      description: feature.description,
      type: "feature" as TaskType,
      project: spec.project_name,
      acceptance_criteria: feature.requirements
    }));
    
    // Create all tasks in one batch
    const result = await mcp.bulk_create_tasks({ tasks });
    
    // Link tasks that have external dependencies
    for (const task of result.created_tasks) {
      if (task.external_dependencies) {
        await this.linkExternalDependencies(task.id, task.external_dependencies);
      }
    }
    
    return {
      created_tasks: result.created_tasks,
      git_commit: result.git_commit
    };
  }
}
```

#### Use Case 2: GitHub Integration Agent
```typescript
class GitHubSyncAgent {
  async syncTaskWithGitHubIssue(taskId: string, repoUrl: string, issueNumber: number) {
    // Parse GitHub reference
    const githubRef = this.formatGitHubRef(repoUrl, issueNumber);
    
    // Link the task to GitHub issue
    await mcp.link_external_ticket({
      task_id: taskId,
      external_ref: githubRef,
      relationship_type: "implements"
    });
    
    // Get task context for updates
    const context = await mcp.get_task_history({ task_id: taskId });
    
    return {
      linked_reference: githubRef,
      task_history: context.history
    };
  }
  
  async findTasksForGitHubIssues(project: string): Promise<SyncReport> {
    // Get all external GitHub links
    const links = await mcp.get_external_links({
      project,
      system: "github"
    });
    
    // Get project context
    const context = await mcp.get_project_context({ project });
    
    return this.generateSyncReport(links, context);
  }
}
```

### CLI Integration for External Links

```bash
# Link task to external systems
lotar task link AUTH-001 --github="myorg/frontend#456" --type="implements"
lotar task link AUTH-001 --jira="PROJ-789" --type="depends-on"
lotar task link AUTH-001 --linear="LIN-123" --type="references"

# Show external links
lotar task show AUTH-001 --include-external
lotar task links --project="webapp" --system="github"

# Validate external references
lotar external validate "github:myorg/repo#123"
lotar external list --project="webapp" --system="jira"
```

### Web Interface Integration

```javascript
// External links component
const ExternalLinksEditor = ({ task, onChange }) => {
  const [externalLinks, setExternalLinks] = useState(task.relationships.external_links || {});
  
  const addExternalLink = (system, reference, relationship) => {
    const updated = {
      ...externalLinks,
      [relationship]: [...(externalLinks[relationship] || []), `${system}:${reference}`]
    };
    setExternalLinks(updated);
    onChange(updated);
  };
  
  return (
    <div className="external-links">
      <h4>External System Links</h4>
      
      {/* GitHub integration */}
      <ExternalSystemInput 
        system="github"
        placeholder="org/repo#123 or #123"
        onAdd={(ref, type) => addExternalLink("github", ref, type)}
      />
      
      {/* Jira integration */}
      <ExternalSystemInput 
        system="jira"
        placeholder="PROJ-123"
        onAdd={(ref, type) => addExternalLink("jira", ref, type)}
      />
      
      {/* Display current links */}
      <ExternalLinksList 
        links={externalLinks}
        onRemove={removeExternalLink}
      />
    </div>
  );
};
```

### Acceptance Criteria
- ✅ AI agents can perform all task operations efficiently
- ✅ External system integration works with major platforms
- ✅ Batch operations maintain git integrity
- ✅ External references are validated and parsed correctly
- ✅ CLI and web interface support external linking
- ✅ Git history includes external link changes
- ✅ MCP interface is simple and well-documented

### Benefits for AI Agents
1. **Simple Operations**: Focus on task management without NLP complexity
2. **Batch Efficiency**: Multiple operations in single git commits
3. **External Integration**: Connect tasks to existing workflows
4. **Context Awareness**: Access to git history and project context
5. **Standard Protocol**: Use established MCP standards

This simplified approach makes LoTaR AI-agent friendly while focusing on practical integration needs rather than complex AI features.
