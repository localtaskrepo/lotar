# MCP Integration Specification (Simplified)

*Last Updated: 2025-07-29*

## Overview

This specification defines the Model Context Protocol (MCP) integration for LoTaR, providing AI agents with efficient task management capabilities without complex NLP features.

## Vision Statement

**AI agents as first-class citizens** - LoTaR provides a clean MCP interface that enables AI agents to manage tasks efficiently, leveraging their existing language understanding capabilities rather than building redundant NLP features.

## MCP Server Architecture

### Core Components

```rust
// Simplified MCP server structure
pub struct LoTaRMCPServer {
    // Core data access
    repository: Arc<RwLock<LoTaRRepository>>,
    
    // Configuration
    config: MCPServerConfig,
}

pub struct MCPServerConfig {
    pub enabled_tools: HashSet<String>,
    pub max_batch_size: usize,
    pub rate_limit: u32,
}
```

## MCP Tool Categories (Phase 1)

### Category 1: Task Management Tools
**Purpose**: Efficient CRUD operations optimized for AI workflows

```typescript
interface TaskManagementTools {
  // Basic operations
  create_task(params: CreateTaskParams): Promise<CreateTaskResult>;
  update_task(params: UpdateTaskParams): Promise<UpdateTaskResult>;
  get_task(params: GetTaskParams): Promise<GetTaskResult>;
  list_tasks(params: ListTasksParams): Promise<ListTasksResult>;
  delete_task(params: DeleteTaskParams): Promise<DeleteTaskResult>;
  
  // Batch operations for efficiency
  bulk_update_tasks(params: BulkUpdateParams): Promise<BulkUpdateResult>;
  bulk_create_tasks(params: BulkCreateParams): Promise<BulkCreateResult>;
  
  // Relationship management with external system support
  add_relationship(params: AddRelationshipParams): Promise<RelationshipResult>;
  remove_relationship(params: RemoveRelationshipParams): Promise<RelationshipResult>;
  get_related_tasks(params: GetRelatedParams): Promise<RelatedTasksResult>;
  
  // External system integration
  link_external_ticket(params: LinkExternalParams): Promise<LinkResult>;
  get_external_links(params: GetExternalParams): Promise<ExternalLinksResult>;
}
```

### Category 2: Git History and Context Tools
**Purpose**: Leverage git history for decision context

```typescript
interface GitContextTools {
  // Git history analysis
  get_task_history(params: {
    task_id: string;
    include_diffs?: boolean;
  }): Promise<{
    history: GitHistoryEntry[];
    decision_timeline: DecisionEvent[];
  }>;
  
  // Project context
  get_project_context(params: {
    project: string;
    include_stats?: boolean;
  }): Promise<{
    project_info: ProjectInfo;
    task_summary: TaskSummary;
    recent_activity: ActivityEvent[];
  }>;
  
  // Find similar tasks based on content similarity
  find_similar_tasks(params: {
    task_id?: string;
    title?: string;
    description?: string;
    project?: string;
    limit?: number;
  }): Promise<{
    similar_tasks: SimilarTask[];
  }>;
}
```

## Enhanced Relationship System with External Tickets

### External Ticket Prefixes
```typescript
// Support for external system references
type ExternalTicketRef = 
  | `github:${string}/${string}#${number}`     // github:org/repo#123
  | `github:${string}#${number}`               // github:repo#123 (current org)
  | `github:#${number}`                        // github:#123 (current repo)
  | `jira:${string}-${number}`                 // jira:PROJ-123
  | `linear:${string}`                         // linear:LIN-123
  | `azure:${number}`                          // azure:12345
  | `asana:${number}`;                         // asana:12345

interface EnhancedTaskRelationships {
  // Internal task relationships
  depends_on: string[];
  blocks: string[];
  related: string[];
  parent: string[];
  child: string[];
  fixes: string[];
  duplicate_of: string[];
  
  // External ticket relationships
  external_links: {
    depends_on: ExternalTicketRef[];
    blocks: ExternalTicketRef[];
    related: ExternalTicketRef[];
    implements: ExternalTicketRef[];    // This task implements external requirement
    references: ExternalTicketRef[];    // General reference
  };
}
```

### External Link Examples
```yaml
# Task file with external links
relationships:
  depends_on: ["AUTH-002"]
  external_links:
    implements: ["github:org/frontend#456", "jira:PROJ-789"]
    depends_on: ["github:#123"]  # Current repo issue
    references: ["linear:LIN-456"]
```

## Simplified AI Agent Use Cases

### Use Case 1: Task Management Assistant
```typescript
class TaskManager {
  async createTasksFromList(descriptions: string[], project: string) {
    // AI agent creates multiple tasks efficiently
    const tasks = descriptions.map(desc => ({
      title: this.extractTitle(desc),
      description: desc,
      project: project,
      type: this.inferType(desc),
      priority: this.inferPriority(desc)
    }));
    
    return await mcp.bulk_create_tasks({ tasks });
  }
  
  async linkToGitHubIssue(taskId: string, repoUrl: string, issueNumber: number) {
    // Link LoTaR task to GitHub issue
    const githubRef = this.formatGitHubRef(repoUrl, issueNumber);
    return await mcp.link_external_ticket({
      task_id: taskId,
      external_ref: githubRef,
      relationship_type: "implements"
    });
  }
}
```

### Use Case 2: Project Synchronization Agent
```typescript
class ProjectSync {
  async syncWithGitHub(project: string, repoUrl: string) {
    // Get current tasks
    const tasks = await mcp.list_tasks({ project });
    
    // Find GitHub links
    const githubLinks = await mcp.get_external_links({
      project,
      system: "github"
    });
    
    // Sync status or create reports
    return this.generateSyncReport(tasks, githubLinks);
  }
}
```

## Phase 2: Advanced Analytics (Future)

Future analytics features (not in initial implementation):
- Project risk analysis
- Timeline prediction
- Decision pattern analysis
- Resource optimization
- Process improvement suggestions

These will be added later as separate tools available through CLI, web interface, and MCP.

## Implementation Requirements

### MCP Protocol Implementation
```rust
impl MCPServer for LoTaRMCPServer {
    fn get_tools(&self) -> Vec<MCPTool> {
        vec![
            // Core task management
            MCPTool::new("create_task", "Create a new task"),
            MCPTool::new("update_task", "Update an existing task"),
            MCPTool::new("list_tasks", "List tasks with filtering"),
            MCPTool::new("bulk_update_tasks", "Update multiple tasks"),
            
            // External integration
            MCPTool::new("link_external_ticket", "Link to external ticket"),
            MCPTool::new("get_external_links", "Get external ticket links"),
            
            // Git context
            MCPTool::new("get_task_history", "Get task change history"),
            MCPTool::new("find_similar_tasks", "Find similar tasks"),
            MCPTool::new("get_project_context", "Get project overview"),
        ]
    }
}
```

### External System Integration
```rust
pub struct ExternalSystemManager {
    github_config: Option<GitHubConfig>,
    jira_config: Option<JiraConfig>,
    // Other systems...
}

impl ExternalSystemManager {
    pub fn parse_external_ref(&self, reference: &str) -> Result<ExternalTicket> {
        if reference.starts_with("github:") {
            self.parse_github_ref(reference)
        } else if reference.starts_with("jira:") {
            self.parse_jira_ref(reference)
        } else {
            Err(LoTaRError::InvalidExternalRef(reference.to_string()))
        }
    }
    
    pub async fn validate_external_ref(&self, reference: &str) -> Result<bool> {
        // Optional: validate that external ticket exists
        // This would require API access to external systems
    }
}
```

## Security and Configuration

### Simple Configuration
```yaml
# mcp-config.yaml
mcp:
  enabled: true
  port: 3000
  max_concurrent_agents: 5
  
  tools:
    task_management: true
    git_context: true
    external_links: true
  
  external_systems:
    github:
      enabled: true
      default_org: "myorg"
      default_repo: "myrepo"
    jira:
      enabled: true
      base_url: "https://mycompany.atlassian.net"
  
  security:
    require_api_key: true
    rate_limit: 100  # requests per minute
```

## Success Metrics

### Adoption Metrics
- Number of AI agents using MCP interface
- Task operations per minute through MCP
- External system integration usage

### Quality Metrics
- Response time for basic operations (target: <100ms)
- Batch operation efficiency
- External link validation accuracy

This simplified specification focuses on the core value: making LoTaR accessible to AI agents while maintaining the git-native approach and adding practical external system integration.
