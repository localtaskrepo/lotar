<template>
  <section class="col sprints-page" style="gap: 16px;">
    <header class="row header">
      <div class="col" style="gap: 4px;">
        <h1>Sprints</h1>
      </div>
      <div class="row split-actions" style="gap: 8px; align-items: center; flex-wrap: wrap;">
        <UiButton
          icon-only
          type="button"
          aria-label="Clear sprint filters"
          title="Clear sprint filters"
          :disabled="!filtersActive"
          @click="clearFilters"
        >
          <IconGlyph name="close" />
        </UiButton>
        <ReloadButton
          :disabled="busy"
          :loading="busy"
          label="Refresh sprints"
          title="Refresh sprints"
          @click="handleManualRefresh"
        />
      </div>
    </header>

    <div v-if="hasMissingSprints" class="alert warn">
      <p>
        Missing sprint files detected for <strong>{{ missingSprintMessage }}</strong>. Review assignments or run cleanup before creating new sprints.
      </p>
    </div>

    <Teleport to="body">
      <div
        v-if="modal.open"
        class="sprint-modal__overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="modal.mode === 'create' ? 'Create sprint' : 'Edit sprint'"
        @click.self="closeModal"
      >
        <UiCard class="sprint-modal__card">
          <form class="col sprint-modal__form" @submit.prevent="submitModal">
            <header class="row sprint-modal__header">
              <div class="col" style="gap: 4px;">
                <h2>{{ modal.mode === 'create' ? 'Create sprint' : 'Edit sprint' }}</h2>
                <p class="muted">
                  {{ modal.mode === 'create'
                    ? 'Capture scheduling details and capacity to keep planning accurate.'
                    : 'Update plan metadata, timing, or capacity for this sprint.' }}
                </p>
              </div>
              <UiButton
                icon-only
                variant="ghost"
                type="button"
                aria-label="Close dialog"
                :disabled="submitting"
                @click="closeModal"
              >
                <IconGlyph name="close" />
              </UiButton>
            </header>
            <div class="row" style="gap: 12px; flex-wrap: wrap;">
              <label class="col" style="gap: 4px; min-width: 200px;">
                <span class="muted">Label<span class="muted"> *</span></span>
                <input ref="firstField" class="input" v-model="form.label" placeholder="Sprint name" required />
              </label>
              <label class="col" style="gap: 4px; min-width: 200px;">
                <span class="muted">Goal</span>
                <input class="input" v-model="form.goal" placeholder="Optional goal" />
              </label>
            </div>
            <div class="row" style="gap: 12px; flex-wrap: wrap;">
              <label class="col" style="gap: 4px; min-width: 200px;">
                <span class="muted">Planned start</span>
                <input class="input" type="datetime-local" step="60" v-model="form.starts_at" />
              </label>
              <label class="col" style="gap: 4px; min-width: 200px;">
                <span class="muted">Planned end</span>
                <input class="input" type="datetime-local" step="60" v-model="form.ends_at" />
              </label>
              <label class="col" style="gap: 4px; min-width: 160px;">
                <span class="muted">Length</span>
                <input class="input" v-model="form.plan_length" placeholder="2w, 10d" />
              </label>
            </div>
            <div class="row" style="gap: 12px; flex-wrap: wrap;">
              <label class="col" style="gap: 4px; min-width: 150px;">
                <span class="muted">Capacity (points)</span>
                <input class="input" type="number" inputmode="numeric" v-model="form.capacity_points" />
              </label>
              <label class="col" style="gap: 4px; min-width: 150px;">
                <span class="muted">Capacity (hours)</span>
                <input class="input" type="number" inputmode="numeric" v-model="form.capacity_hours" />
              </label>
              <label class="col" style="gap: 4px; min-width: 160px;">
                <span class="muted">Overdue after</span>
                <input class="input" v-model="form.overdue_after" placeholder="12h" />
              </label>
            </div>
            <label class="col" style="gap: 4px;">
              <span class="muted">Notes</span>
              <textarea class="input" rows="3" v-model="form.notes" placeholder="Additional context"></textarea>
            </label>
            <label v-if="modal.mode === 'create'" class="row" style="gap: 6px; align-items: center;">
              <input type="checkbox" v-model="form.skip_defaults" />
              Skip applying sprint defaults for this sprint
            </label>
            <footer class="row" style="gap: 8px; flex-wrap: wrap;">
              <UiButton variant="primary" type="submit" :disabled="submitting">
                {{ submitting ? (modal.mode === 'create' ? 'Creating…' : 'Saving…') : modal.mode === 'create' ? 'Create sprint' : 'Save changes' }}
              </UiButton>
              <ReloadButton
                :disabled="submitting || busy"
                :loading="busy"
                label="Refresh sprint list"
                title="Refresh sprint list"
                @click="handleManualRefresh"
              />
            </footer>
          </form>
        </UiCard>
      </div>
    </Teleport>

    <Teleport to="body">
      <div
        v-if="deleteDialog.open"
        class="sprint-modal__overlay sprint-delete__overlay"
        role="dialog"
        aria-modal="true"
        :aria-label="deleteDialogTitle"
        @click.self="closeDeleteDialog"
      >
        <UiCard class="sprint-delete__card">
          <div class="col sprint-delete__content">
            <header class="row sprint-delete__header">
              <div class="col" style="gap: 4px;">
                <h2>{{ deleteDialogTitle }}</h2>
                <p class="muted">This action cannot be undone.</p>
              </div>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close dialog"
                title="Close dialog"
                :disabled="deleteDialogSubmitting"
                @click="closeDeleteDialog"
              >
                <IconGlyph name="close" />
              </UiButton>
            </header>
            <div class="col sprint-delete__body">
              <p>Deleting this sprint removes its board card and clears memberships from affected tasks.</p>
              <p v-if="deleteDialog.backlogCount">
                {{ deleteDialog.backlogCount }} task{{ deleteDialog.backlogCount === 1 ? '' : 's' }} will return to the backlog.
              </p>
            </div>
            <footer class="row sprint-delete__actions">
              <UiButton variant="danger" type="button" :disabled="deleteDialogSubmitting" @click="confirmDeleteSprint">
                {{ deleteDialogSubmitting ? 'Deleting…' : 'Delete sprint' }}
              </UiButton>
              <UiButton variant="ghost" type="button" :disabled="deleteDialogSubmitting" @click="closeDeleteDialog">Cancel</UiButton>
            </footer>
          </div>
        </UiCard>
      </div>
    </Teleport>

    <SprintAnalyticsDialog
      :open="analyticsModal.open"
      :sprints="sortedSprints"
      :selected-sprint-id="selectedAnalyticsSprintId"
      :summary="analyticsSummary"
  :summary-loading="summaryLoading"
  :summary-error="summaryError"
      :burndown="analyticsBurndown"
      :velocity="analyticsVelocity"
      :loading="analyticsLoading"
      :error="analyticsError"
      :velocity-loading="velocityLoading"
      :velocity-error="velocityError"
      :velocity-window-size="velocityWindowSize"
      :velocity-focus-sprint-ids="velocityFocusSprintIds"
      :active-tab="analyticsModal.tab"
      :burndown-metric="analyticsModal.burndownMetric"
      :velocity-metric="analyticsModal.velocityMetric"
      @close="closeAnalytics"
      @refresh="refreshAnalytics"
      @update:selected-sprint-id="selectAnalyticsSprint"
      @update:active-tab="setAnalyticsTab"
      @update:burndown-metric="setBurndownMetric"
      @update:velocity-metric="setVelocityMetric"
    />

    <div class="filter-card">
      <SmartListChips
        :statuses="statusOptions"
        :priorities="priorityOptions"
        :value="filter"
        :custom-presets="customFilterPresets"
        @update:value="onChipsUpdate"
        @preset="handleCustomPreset"
      />
      <FilterBar
        ref="filterBarRef"
        :statuses="statuses"
        :priorities="priorities"
        :types="types"
        :value="filter"
        storage-key="lotar.sprints.filter"
        @update:value="onFilterUpdate"
      />
      <div class="filter-meta">
        <div class="filter-meta__primary">
          <label class="filter-field">
            <span class="muted">Sprint window</span>
            <UiSelect v-model="timeRange">
              <option v-for="option in timeRangeChoices" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </UiSelect>
          </label>
          <label
            v-if="showAllowClosedControl"
            class="filter-checkbox"
          >
            <input type="checkbox" v-model="allowClosed" />
            Allow editing closed sprints
          </label>
          <label class="filter-checkbox">
            <input type="checkbox" v-model="highlightMultiSprint" />
            Highlight tasks in multiple sprints
          </label>
          <div class="filter-meta__actions">
            <UiButton
              ref="columnMenuButtonRef"
              type="button"
              class="filter-columns-btn"
              title="Configure columns"
              @click="toggleColumnMenu"
            >
              <IconGlyph name="columns" aria-hidden="true" />
              <span>Columns</span>
            </UiButton>
            <UiButton
              class="create-sprint-button"
              type="button"
              aria-label="Create sprint"
              title="Create sprint"
              @click="openCreate"
            >
              <IconGlyph name="plus" aria-hidden="true" />
              <span>Sprint</span>
            </UiButton>
          </div>
        </div>
      </div>
      <div v-if="columnMenuOpen" ref="columnMenuRef" class="columns-popover card">
        <div class="col" style="gap: 8px;">
          <label v-for="col in allColumns" :key="col" class="row" style="gap: 6px; align-items: center;">
            <input
              type="checkbox"
              :checked="columnsSet.has(col)"
              :disabled="col === 'id' || col === 'title'"
              @change="toggleColumn(col, $event)"
            />
            <span>{{ headerLabel(col) }}</span>
          </label>
          <div class="row" style="gap: 8px; flex-wrap: wrap;">
            <UiButton type="button" @click="resetColumns">Reset</UiButton>
            <UiButton variant="ghost" type="button" @click="closeColumnMenu">Close</UiButton>
          </div>
        </div>
      </div>
    </div>

    <p v-if="hiddenClosedCount" class="muted hint">
      Hiding {{ hiddenClosedCount }} completed sprint{{ hiddenClosedCount === 1 ? '' : 's' }} outside the selected window. Choose "All time" to include every sprint.
    </p>

    <UiLoader v-if="!initialized" size="md" />

    <template v-else>
      <UiEmptyState
        v-if="!visibleSprints.length && !sortedBacklogTasks.length"
        title="No sprints yet"
        description="Create a sprint to start planning work."
        primary-label="Create sprint"
        @primary="openCreate"
      />

      <div class="col group-stack" style="gap: 12px;">
        <UiCard
          v-for="sprint in visibleSprints"
          :key="sprint.id"
          class="sprint-group"
          :class="{ 'sprint-group--drop': hoverSprintId === sprint.id }"
          :data-sprint-id="sprint.id"
          @dragenter.prevent="onSprintDragEnter($event, sprint.id)"
          @dragleave="onSprintDragLeave(sprint.id)"
          @dragover.prevent="onSprintDragOver"
          @drop.prevent="onSprintDrop($event, sprint.id)"
        >
          <header class="group-header">
            <button class="collapse-btn" type="button" :aria-expanded="isExpanded(sectionKey(sprint.id))" @click="toggleSection(sectionKey(sprint.id))">
              <span aria-hidden="true">{{ isExpanded(sectionKey(sprint.id)) ? '▾' : '▸' }}</span>
            </button>
            <div class="group-title">
              <div class="group-title__heading">
                <h2>{{ sprint.display_name }}</h2>
                <span class="badge" :class="badgeClass(sprint.state)">{{ sprint.state }}</span>
              </div>
              <div class="group-meta">
                <div class="meta-item">
                  <span class="meta-label">ID</span>
                  <span class="meta-value">#{{ sprint.id }}</span>
                </div>
                <template v-if="sprint.actual_start">
                  <div class="meta-item">
                    <span class="meta-label">Started</span>
                    <span class="meta-value" :title="formatRelative(sprint.actual_start)">
                      {{ formatShortDate(sprint.actual_start) || formatRelative(sprint.actual_start) }}
                    </span>
                  </div>
                </template>
                <template v-else-if="sprint.planned_start">
                  <div class="meta-item">
                    <span class="meta-label">Planned start</span>
                    <span class="meta-value" :title="formatRelative(sprint.planned_start)">
                      {{ formatShortDate(sprint.planned_start) }}
                    </span>
                  </div>
                </template>
                <div
                  v-if="startDelayLookup.get(sprint.id)"
                  :class="['meta-item', startDelayLookup.get(sprint.id)?.severity === 'danger' ? 'meta-item--danger' : 'meta-item--warning']"
                >
                  <span class="meta-label">Start overdue</span>
                  <span
                    class="meta-value"
                    :class="startDelayLookup.get(sprint.id)?.severity === 'danger' ? 'meta-value--danger' : 'meta-value--warning'"
                  >
                    {{ startDelayLookup.get(sprint.id)?.elapsed }} late
                    <span v-if="startDelayLookup.get(sprint.id)?.limitLabel">
                      (limit {{ startDelayLookup.get(sprint.id)?.limitLabel }})
                    </span>
                  </span>
                </div>
                <template v-if="sprint.actual_end">
                  <div class="meta-item">
                    <span class="meta-label">Completed</span>
                    <span class="meta-value" :title="formatRelative(sprint.actual_end)">
                      {{ formatShortDate(sprint.actual_end) || formatRelative(sprint.actual_end) }}
                    </span>
                  </div>
                </template>
                <template v-else-if="sprint.planned_end">
                  <div class="meta-item">
                    <span class="meta-label">Planned end</span>
                    <span class="meta-value" :title="formatRelative(sprint.planned_end)">
                      {{ formatShortDate(sprint.planned_end) }}
                    </span>
                  </div>
                </template>
                <div v-if="sprint.plan_length" class="meta-item">
                  <span class="meta-label">Length</span>
                  <span class="meta-value">{{ sprint.plan_length }}</span>
                </div>
                <div v-if="capacitySummary(sprint)" class="meta-item">
                  <span class="meta-label">Capacity</span>
                  <span class="meta-value">{{ capacitySummary(sprint) }}</span>
                </div>
                <div v-if="overdueLimitLookup.get(sprint.id)" class="meta-item">
                  <span class="meta-label">Overdue</span>
                  <span class="meta-value">{{ overdueLimitLookup.get(sprint.id) }}</span>
                </div>
                <template v-if="sprint.goal">
                  <div
                    class="meta-item meta-item--hover meta-item--truncate"
                    :data-hover="sprint.goal"
                    :title="sprint.goal"
                    :aria-label="`Goal: ${sprint.goal}`"
                    tabindex="0"
                  >
                    <span class="meta-label">Goal</span>
                    <span class="meta-value meta-value--truncate">{{ sprint.goal }}</span>
                  </div>
                </template>
                <template v-else-if="!sprint.planned_start && !sprint.actual_start && sprint.created">
                  <div class="meta-item">
                    <span class="meta-label">Created</span>
                    <span class="meta-value">{{ formatRelative(sprint.created) }}</span>
                  </div>
                </template>
                <div
                  v-if="sprint.notes"
                  class="meta-item meta-item--hover meta-item--notes"
                  :data-hover="sprint.notes"
                  :title="sprint.notes"
                  :aria-label="`Notes: ${sprint.notes}`"
                  tabindex="0"
                >
                  <span class="meta-label">Notes</span>
                  <span class="meta-value meta-value--icon" aria-hidden="true">
                    <svg
                      class="meta-icon"
                      width="16"
                      height="16"
                      viewBox="0 0 16 16"
                      xmlns="http://www.w3.org/2000/svg"
                      focusable="false"
                    >
                      <path
                        d="M3 1.5a.5.5 0 0 1 .5-.5h6.586a.5.5 0 0 1 .354.146l2.914 2.914a.5.5 0 0 1 .146.354V14.5a.5.5 0 0 1-.5.5H3.5a.5.5 0 0 1-.5-.5Z"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.1"
                        stroke-linejoin="round"
                      />
                      <path
                        d="M10 1.5V4a.5.5 0 0 0 .5.5H13"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.1"
                        stroke-linecap="round"
                        stroke-linejoin="round"
                      />
                      <path
                        d="M5 7.25h5.5M5 9.75h5.5M5 12.25h3.5"
                        fill="none"
                        stroke="currentColor"
                        stroke-width="1.1"
                        stroke-linecap="round"
                      />
                    </svg>
                  </span>
                </div>
              </div>
            </div>
            <span class="group-count">{{ getTasksForSprint(sprint.id).length }} task{{ getTasksForSprint(sprint.id).length === 1 ? '' : 's' }}</span>
            <div class="group-actions">
              <UiLoader v-if="isLifecycleBusy(sprint.id)" size="sm" />
              <UiButton
                v-if="canStartSprint(sprint)"
                class="small"
                type="button"
                :disabled="isLifecycleBusy(sprint.id)"
                @click="startSprint(sprint)"
              >
                Start
              </UiButton>
              <UiButton
                v-if="canCompleteSprint(sprint)"
                class="small"
                type="button"
                :disabled="isLifecycleBusy(sprint.id)"
                @click="completeSprint(sprint)"
              >
                Complete
              </UiButton>
              <UiButton
                v-if="canReopenSprint(sprint)"
                class="small"
                type="button"
                :disabled="isLifecycleBusy(sprint.id)"
                @click="reopenSprint(sprint)"
              >
                Reopen
              </UiButton>
              <UiButton class="small" type="button" @click="openAnalytics(sprint)">
                Insights
              </UiButton>
              <UiButton
                icon-only
                type="button"
                aria-label="Edit sprint"
                title="Edit sprint"
                @click="openEdit(sprint)"
              >
                <IconGlyph name="edit" />
              </UiButton>
              <UiButton
                class="sprint-delete-button"
                icon-only
                type="button"
                aria-label="Delete sprint"
                title="Delete sprint"
                data-testid="sprint-delete"
                :disabled="deleteDialogSubmitting"
                @click="openDelete(sprint)"
              >
                <IconGlyph name="trash" />
              </UiButton>
            </div>
          </header>
          <transition name="collapse">
            <div v-show="isExpanded(sectionKey(sprint.id))" class="group-body">
              <p v-if="!getSortedTasksForSprint(sprint.id).length" class="muted empty-placeholder">No tasks in this sprint.</p>
              <div v-else class="table-wrapper">
                <table class="sprint-table">
                  <thead>
                    <tr>
                      <th
                        v-for="col in visibleColumns"
                        :key="col"
                        :class="['sortable', { active: sort.key === col }]"
                        :aria-sort="ariaSort(col)"
                        :data-column="col"
                      >
                        <button class="header-button" type="button" @click="onSort(col)">
                          {{ headerLabel(col) }}
                          <span class="sort-glyph" aria-hidden="true">{{ sortGlyph(col) }}</span>
                        </button>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="task in getSortedTasksForSprint(sprint.id)"
                      :key="task.id + '-' + sprint.id"
                      class="task-row"
                      :class="{
                        'task-row--dragging': draggingTaskId === task.id,
                      }"
                      draggable="true"
                      :data-task-id="task.id"
                      @dragstart="onTaskDragStart($event, task.id, sprint.id)"
                      @dragend="onTaskDragEnd"
                      @click="onTaskRowClick(task, $event)"
                    >
                      <td v-for="col in visibleColumns" :key="col" :class="['task-cell', `task-cell--${col}`]">
                        <template v-if="col === 'id'">
                          <span class="muted">{{ projectOf(task.id) }}</span>
                          <strong>{{ numericOf(task.id) }}</strong>
                        </template>
                        <template v-else-if="col === 'title'">
                          <span class="task-title">
                            {{ task.title || 'Untitled task' }}
                            <span
                              v-if="highlightMultiSprint && multiSprintTaskIds.has(task.id)"
                              class="task-title__badge badge badge--info"
                              :title="multiSprintTooltip(task, sprint.id)"
                              :aria-label="multiSprintTooltip(task, sprint.id)"
                            >
                              Multi
                            </span>
                          </span>
                        </template>
                        <template v-else-if="col === 'status'">
                          <span class="status" :data-status="task.status">{{ task.status }}</span>
                        </template>
                        <template v-else-if="col === 'priority'">
                          <span>{{ task.priority || '—' }}</span>
                        </template>
                        <template v-else-if="col === 'task_type'">
                          <span>{{ task.task_type || '—' }}</span>
                        </template>
                        <template v-else-if="col === 'reporter'">
                          <span v-if="task.reporter">@{{ task.reporter }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'assignee'">
                          <span v-if="task.assignee">@{{ task.assignee }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'effort'">
                          <span v-if="(task as any).effort">{{ (task as any).effort }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'tags'">
                          <div class="row" style="gap: 6px; flex-wrap: wrap; align-items: center;">
                            <span v-for="tag in task.tags" :key="task.id + '-tag-' + tag" class="chip small">{{ tag }}</span>
                            <span v-if="!task.tags.length" class="muted">—</span>
                          </div>
                        </template>
                        <template v-else-if="col === 'sprints'">
                          <div class="row" style="gap: 6px; flex-wrap: wrap; align-items: center;">
                            <span
                              v-for="sprintId in task.sprints"
                              :key="task.id + '-sprint-' + sprintId"
                              class="chip small"
                              :class="sprintStateClass(sprintId)"
                            >
                              {{ sprintLabel(sprintId) }}
                            </span>
                            <span v-if="!task.sprints.length" class="muted">—</span>
                          </div>
                        </template>
                        <template v-else-if="col === 'due_date'">
                          <span
                            v-if="formatDue(task.due_date)"
                            :class="{ 'text-overdue': isTaskOverdue(task) }"
                          >
                            {{ formatDue(task.due_date) }}
                          </span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'created'">
                          <span :title="formatExact(task.created)">{{ formatRelative(task.created) }}</span>
                        </template>
                        <template v-else-if="col === 'modified'">
                          <span :title="formatExact(task.modified)">{{ formatRelative(task.modified) }}</span>
                        </template>
                        <template v-else>
                          <span>{{ (task as any)[col] ?? '—' }}</span>
                        </template>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          </transition>
        </UiCard>

        <UiCard
          ref="backlogRef"
          id="backlog"
          class="sprint-group backlog-group"
          :class="{ 'sprint-group--drop': hoverBacklog }"
          data-sprint-id="backlog"
          @dragenter.prevent="onBacklogDragEnter($event)"
          @dragleave="onBacklogDragLeave"
          @dragover.prevent="onBacklogDragOver($event)"
          @drop.prevent="onBacklogDrop($event)"
        >
          <header class="group-header">
            <button class="collapse-btn" type="button" :aria-expanded="isExpanded('backlog')" @click="toggleSection('backlog')">
              <span aria-hidden="true">{{ isExpanded('backlog') ? '▾' : '▸' }}</span>
            </button>
            <div class="group-title">
              <h2>Backlog</h2>
              <div class="group-meta">
                <span class="muted">Unassigned tasks</span>
              </div>
            </div>
            <span class="group-count">{{ sortedBacklogTasks.length }} task{{ sortedBacklogTasks.length === 1 ? '' : 's' }}</span>
          </header>
          <transition name="collapse">
            <div v-show="isExpanded('backlog')" class="group-body">
              <p v-if="!sortedBacklogTasks.length" class="muted empty-placeholder">Backlog is clear.</p>
              <div v-else class="table-wrapper">
                <table class="sprint-table">
                  <thead>
                    <tr>
                      <th
                        v-for="col in visibleColumns"
                        :key="col"
                        :class="['sortable', { active: sort.key === col }]"
                        :aria-sort="ariaSort(col)"
                        :data-column="col"
                      >
                        <button class="header-button" type="button" @click="onSort(col)">
                          {{ headerLabel(col) }}
                          <span class="sort-glyph" aria-hidden="true">{{ sortGlyph(col) }}</span>
                        </button>
                      </th>
                    </tr>
                  </thead>
                  <tbody>
                    <tr
                      v-for="task in sortedBacklogTasks"
                      :key="task.id"
                      class="task-row"
                      :class="{
                        'task-row--dragging': draggingTaskId === task.id,
                      }"
                      draggable="true"
                      :data-task-id="task.id"
                      @dragstart="onTaskDragStart($event, task.id, null)"
                      @dragend="onTaskDragEnd"
                      @click="onTaskRowClick(task, $event)"
                    >
                      <td v-for="col in visibleColumns" :key="col" :class="['task-cell', `task-cell--${col}`]">
                        <template v-if="col === 'id'">
                          <span class="muted">{{ projectOf(task.id) }}</span>
                          <strong>{{ numericOf(task.id) }}</strong>
                        </template>
                        <template v-else-if="col === 'title'">
                          <span class="task-title">
                            {{ task.title || 'Untitled task' }}
                            <span
                              v-if="highlightMultiSprint && multiSprintTaskIds.has(task.id)"
                              class="task-title__badge badge badge--info"
                              :title="multiSprintTooltip(task, null)"
                              :aria-label="multiSprintTooltip(task, null)"
                            >
                              Multi
                            </span>
                          </span>
                        </template>
                        <template v-else-if="col === 'status'">
                          <span class="status" :data-status="task.status">{{ task.status }}</span>
                        </template>
                        <template v-else-if="col === 'priority'">
                          <span>{{ task.priority || '—' }}</span>
                        </template>
                        <template v-else-if="col === 'task_type'">
                          <span>{{ task.task_type || '—' }}</span>
                        </template>
                        <template v-else-if="col === 'reporter'">
                          <span v-if="task.reporter">@{{ task.reporter }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'assignee'">
                          <span v-if="task.assignee">@{{ task.assignee }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'effort'">
                          <span v-if="(task as any).effort">{{ (task as any).effort }}</span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'tags'">
                          <div class="row" style="gap: 6px; flex-wrap: wrap; align-items: center;">
                            <span v-for="tag in task.tags" :key="task.id + '-tag-' + tag" class="chip small">{{ tag }}</span>
                            <span v-if="!task.tags.length" class="muted">—</span>
                          </div>
                        </template>
                        <template v-else-if="col === 'sprints'">
                          <span class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'due_date'">
                          <span
                            v-if="formatDue(task.due_date)"
                            :class="{ 'text-overdue': isTaskOverdue(task) }"
                          >
                            {{ formatDue(task.due_date) }}
                          </span>
                          <span v-else class="muted">—</span>
                        </template>
                        <template v-else-if="col === 'created'">
                          <span :title="formatExact(task.created)">{{ formatRelative(task.created) }}</span>
                        </template>
                        <template v-else-if="col === 'modified'">
                          <span :title="formatExact(task.modified)">{{ formatRelative(task.modified) }}</span>
                        </template>
                        <template v-else>
                          <span>{{ (task as any)[col] ?? '—' }}</span>
                        </template>
                      </td>
                    </tr>
                  </tbody>
                </table>
              </div>
            </div>
          </transition>
        </UiCard>
      </div>
    </template>
  </section>
</template>

<script setup lang="ts">
import type { ComponentPublicInstance } from 'vue'
import { computed, nextTick, onMounted, onUnmounted, reactive, ref, watch } from 'vue'
import { useRoute } from 'vue-router'
import { api } from '../api/client'
import type {
    SprintBurndownResponse,
    SprintCreateRequest,
    SprintListItem,
    SprintSummaryReportResponse,
    SprintUpdateRequest,
    SprintVelocityResponse,
    TaskDTO,
} from '../api/types'
import FilterBar from '../components/FilterBar.vue'
import IconGlyph from '../components/IconGlyph.vue'
import ReloadButton from '../components/ReloadButton.vue'
import SmartListChips from '../components/SmartListChips.vue'
import UiButton from '../components/UiButton.vue'
import UiCard from '../components/UiCard.vue'
import UiEmptyState from '../components/UiEmptyState.vue'
import UiLoader from '../components/UiLoader.vue'
import UiSelect from '../components/UiSelect.vue'
import SprintAnalyticsDialog from '../components/analytics/SprintAnalyticsDialog.vue'
import { showToast } from '../components/toast'
import { useConfig } from '../composables/useConfig'
import { useCopyModifier } from '../composables/useCopyModifier'
import { DEFAULT_VELOCITY_PARAMS, useSprintAnalytics } from '../composables/useSprintAnalytics'
import { useSprints } from '../composables/useSprints'
import { useTaskPanelController } from '../composables/useTaskPanelController'
import { fromDateTimeInputValue, parseTaskDate, safeTimestamp, startOfLocalDay, toDateTimeInputValue } from '../utils/date'

type ColumnKey =
  | 'id'
  | 'title'
  | 'status'
  | 'priority'
  | 'task_type'
  | 'reporter'
  | 'assignee'
  | 'effort'
  | 'tags'
  | 'sprints'
  | 'due_date'
  | 'created'
  | 'modified'

type SprintMetric = 'tasks' | 'points' | 'hours'
type AnalyticsTab = 'burndown' | 'velocity' | 'health' | 'history'
type TimeRangeKey = 'current' | '30' | '90' | '180' | 'all'

const columnStorageKey = 'lotar.sprints.columns'
const sortStorageKey = 'lotar.sprints.sort'
const timeRangeStorageKey = 'lotar.sprints.window.v2'
const highlightPreferenceStorageKey = 'lotar.sprints.highlightMulti'

const timeRangeChoices: { value: TimeRangeKey; label: string }[] = [
  { value: 'current', label: 'Current sprints' },
  { value: '30', label: 'Last 30 days' },
  { value: '90', label: 'Last 90 days' },
  { value: '180', label: 'Last 180 days' },
  { value: 'all', label: 'All time' },
]

const allColumns: ColumnKey[] = [
  'id',
  'title',
  'status',
  'priority',
  'task_type',
  'reporter',
  'assignee',
  'effort',
  'tags',
  'sprints',
  'due_date',
  'created',
  'modified',
]

const defaultColumns: ColumnKey[] = ['id', 'title', 'status', 'priority', 'assignee', 'due_date', 'modified']

const BUILTIN_QUERY_KEYS = new Set(['q', 'project', 'status', 'priority', 'type', 'assignee', 'tags', 'order', 'due', 'recent', 'needs'])

function normalizeSprintMembership(raw: unknown): number[] {
  if (!Array.isArray(raw)) return []
  const seen = new Set<number>()
  const values: number[] = []
  for (const entry of raw) {
    const parsed = typeof entry === 'number' ? entry : Number(entry)
    if (!Number.isFinite(parsed) || parsed <= 0 || seen.has(parsed)) continue
    seen.add(parsed)
    values.push(parsed)
  }
  values.sort((a, b) => a - b)
  return values
}

function normalizeTaskRecord(task: TaskDTO): TaskDTO {
  const tags = Array.isArray((task as any).tags) ? [...(task.tags as string[])] : []
  const sprints = normalizeSprintMembership((task as any).sprints)
  return {
    ...task,
    tags,
    sprints,
  }
}

function sprintMembershipMatches(previous: TaskDTO | null | undefined, next: TaskDTO): boolean {
  const prevMembership = normalizeSprintMembership(previous ? (previous as any).sprints : undefined)
  const nextMembership = normalizeSprintMembership((next as any).sprints)
  if (prevMembership.length !== nextMembership.length) return false
  for (let index = 0; index < prevMembership.length; index += 1) {
    if (prevMembership[index] !== nextMembership[index]) {
      return false
    }
  }
  return true
}

function loadStoredColumns(): ColumnKey[] | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage.getItem(columnStorageKey)
    if (!raw) return null
    const parsed = JSON.parse(raw) as unknown
    if (!Array.isArray(parsed)) return null
    const filtered = parsed.filter((item): item is ColumnKey => allColumns.includes(item as ColumnKey))
    if (!filtered.length) return null
    return filtered
  } catch {
    return null
  }
}

function loadStoredSort(): { key: ColumnKey | null; dir: 'asc' | 'desc' } | null {
  if (typeof window === 'undefined') return null
  try {
    const raw = window.localStorage.getItem(sortStorageKey)
    if (!raw) return null
    const parsed = JSON.parse(raw) as { key?: ColumnKey; dir?: 'asc' | 'desc' }
    if (parsed && (!parsed.key || allColumns.includes(parsed.key))) {
      return {
        key: parsed.key ?? null,
        dir: parsed.dir === 'asc' ? 'asc' : 'desc',
      }
    }
    return null
  } catch {
    return null
  }
}

function loadStoredTimeRange(): TimeRangeKey {
  if (typeof window === 'undefined') return 'current'
  try {
    const raw = window.localStorage.getItem(timeRangeStorageKey)
    if (raw === 'current' || raw === '30' || raw === '90' || raw === '180' || raw === 'all') {
      return raw
    }
  } catch {
    /* ignore persistence errors */
  }
  return 'current'
}

function loadHighlightPreference(): boolean {
  if (typeof window === 'undefined') return true
  try {
    const raw = window.localStorage.getItem(highlightPreferenceStorageKey)
    if (raw === 'false') return false
    if (raw === 'true') return true
  } catch {
    /* ignore */
  }
  return true
}

const route = useRoute()

const { sprints, loading: sprintsLoading, refresh: refreshSprints, missingSprints, hasMissing: hasMissingSprints } = useSprints()
const { openTaskPanel } = useTaskPanelController()
const {
  sprintDefaults,
  statuses,
  priorities,
  types,
  customFields: availableCustomFields,
  refresh: refreshConfigDefaults,
} = useConfig()

const sprintAnalytics = useSprintAnalytics()
const highlightMultiSprint = ref(loadHighlightPreference())

watch(highlightMultiSprint, (value) => {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(highlightPreferenceStorageKey, value ? 'true' : 'false')
  } catch {
    /* ignore persistence issues */
  }
})
const {
  copyModifierActive,
  resolveCopyModifier,
  resetCopyModifier,
  bindCopyModifierListeners,
  unbindCopyModifierListeners,
} = useCopyModifier()

const analyticsModal = reactive<{ open: boolean; tab: AnalyticsTab; burndownMetric: SprintMetric; velocityMetric: SprintMetric }>(
  {
    open: false,
    tab: 'health',
    burndownMetric: 'tasks',
    velocityMetric: DEFAULT_VELOCITY_PARAMS.metric as SprintMetric,
  },
)

const VELOCITY_WINDOW = DEFAULT_VELOCITY_PARAMS.limit

const selectedAnalyticsSprintId = ref<number | null>(null)

const modal = reactive<{ open: boolean; mode: 'create' | 'edit'; sprintId: number | null }>({
  open: false,
  mode: 'create',
  sprintId: null,
})

const deleteDialog = reactive<{
  open: boolean
  sprintId: number | null
  label: string
  backlogCount: number
}>({
  open: false,
  sprintId: null,
  label: '',
  backlogCount: 0,
})
const deleteDialogSubmitting = ref(false)
const deleteDialogTitle = computed(() => {
  if (!deleteDialog.open || deleteDialog.sprintId === null) {
    return 'Delete sprint'
  }
  const trimmed = deleteDialog.label.trim()
  if (trimmed.length) {
    return `Delete ${trimmed}`
  }
  return `Delete sprint #${deleteDialog.sprintId}`
})

const submitting = ref(false)
const firstField = ref<HTMLInputElement | null>(null)
const backlogRef = ref<ComponentPublicInstance | null>(null)

const form = reactive({
  label: '',
  goal: '',
  plan_length: '',
  ends_at: '',
  starts_at: '',
  capacity_points: '',
  capacity_hours: '',
  overdue_after: '',
  notes: '',
  skip_defaults: false,
})
const filter = ref<Record<string, string>>({})
const filterBarRef = ref<{ appendCustomFilter: (expr: string) => void; clear?: () => void } | null>(null)
const statusOptions = computed(() => [...(statuses.value || [])])
const priorityOptions = computed(() => [...(priorities.value || [])])
const customFilterPresets = computed(() => {
  const names = (availableCustomFields.value || []).filter((name) => name !== '*')
  return names.slice(0, 6).map((name) => ({
    label: name,
    expression: `field:${name}=`,
  }))
})

function onFilterUpdate(value: Record<string, string>) {
  filter.value = { ...value }
}

function onChipsUpdate(value: Record<string, string>) {
  filter.value = { ...value }
}

function handleCustomPreset(expression: string) {
  filterBarRef.value?.appendCustomFilter(expression)
}

const MS_PER_DAY = 24 * 60 * 60 * 1000

function startOfDay(date: Date) {
  return startOfLocalDay(date)
}

function parseDateLike(value?: string | null) {
  return parseTaskDate(value || undefined)
}

const allowClosed = ref(false)
const timeRange = ref<TimeRangeKey>(loadStoredTimeRange())
const tasks = ref<TaskDTO[]>([])
const tasksLoading = ref(false)
const initialized = ref(false)

const lifecycleBusy = reactive<Record<number, boolean>>({})

function setLifecycleBusy(id: number, busy: boolean) {
  if (busy) {
    lifecycleBusy[id] = true
  } else {
    delete lifecycleBusy[id]
  }
}

function isLifecycleBusy(id: number) {
  return Boolean(lifecycleBusy[id])
}

function canStartSprint(sprint: SprintListItem) {
  if (!sprint) return false
  if (isLifecycleBusy(sprint.id)) return false
  if (sprint.actual_end) return false
  return !sprint.actual_start
}

function canCompleteSprint(sprint: SprintListItem) {
  if (!sprint) return false
  if (isLifecycleBusy(sprint.id)) return false
  if (sprint.actual_end) return false
  return Boolean(sprint.actual_start) || sprint.state === 'active' || sprint.state === 'overdue'
}

function canReopenSprint(sprint: SprintListItem) {
  if (!sprint) return false
  if (isLifecycleBusy(sprint.id)) return false
  return Boolean(sprint.actual_end)
}

async function startSprint(sprint: SprintListItem) {
  if (!canStartSprint(sprint)) return
  setLifecycleBusy(sprint.id, true)
  try {
    const payload: SprintUpdateRequest = {
      sprint: sprint.id,
      actual_started_at: new Date().toISOString(),
    }
    const response = await api.sprintUpdate(payload)
    showToast(`Marked ${response.sprint.display_name} as started`)
    if (response.warnings?.length) {
      response.warnings.forEach((warning) => showToast(`Warning: ${warning}`))
    }
    await refreshAll(true)
  } catch (error: any) {
    showToast(error?.message || 'Failed to start sprint')
  } finally {
    setLifecycleBusy(sprint.id, false)
  }
}

async function completeSprint(sprint: SprintListItem) {
  if (!canCompleteSprint(sprint)) return
  setLifecycleBusy(sprint.id, true)
  try {
    const payload: SprintUpdateRequest = {
      sprint: sprint.id,
      actual_closed_at: new Date().toISOString(),
    }
    const response = await api.sprintUpdate(payload)
    showToast(`Marked ${response.sprint.display_name} as completed`)
    if (response.warnings?.length) {
      response.warnings.forEach((warning) => showToast(`Warning: ${warning}`))
    }
    await refreshAll(true)
  } catch (error: any) {
    showToast(error?.message || 'Failed to complete sprint')
  } finally {
    setLifecycleBusy(sprint.id, false)
  }
}

async function reopenSprint(sprint: SprintListItem) {
  if (!canReopenSprint(sprint)) return
  setLifecycleBusy(sprint.id, true)
  try {
    const payload: SprintUpdateRequest = {
      sprint: sprint.id,
      actual_closed_at: null,
    }
    const response = await api.sprintUpdate(payload)
    showToast(`Reopened ${response.sprint.display_name}`)
    if (response.warnings?.length) {
      response.warnings.forEach((warning) => showToast(`Warning: ${warning}`))
    }
    await refreshAll(true)
  } catch (error: any) {
    showToast(error?.message || 'Failed to reopen sprint')
  } finally {
    setLifecycleBusy(sprint.id, false)
  }
}

const filterTimer = ref<number | null>(null)

const expanded = reactive<Record<string, boolean>>({ backlog: true })

const draggingTaskId = ref<string | null>(null)
const draggingSourceSprint = ref<number | null>(null)
const hoverSprintId = ref<number | null>(null)
const hoverBacklog = ref(false)

const columns = ref<ColumnKey[]>(loadStoredColumns() ?? [...defaultColumns])
const columnsSet = computed(() => new Set(columns.value))
const visibleColumns = computed(() => allColumns.filter((col) => columnsSet.value.has(col)))

watch(
  columns,
  (value) => {
    if (!value.length) {
      columns.value = [...defaultColumns]
      return
    }
    if (typeof window === 'undefined') return
    try {
      window.localStorage.setItem(columnStorageKey, JSON.stringify(value))
    } catch {
      /* ignore persistence errors */
    }
  },
  { deep: true },
)

function headerLabel(col: ColumnKey) {
  const labels: Record<ColumnKey, string> = {
    id: 'ID',
    title: 'Title',
    status: 'Status',
    priority: 'Priority',
    task_type: 'Type',
    reporter: 'Reporter',
    assignee: 'Assignee',
    effort: 'Effort',
    tags: 'Tags',
    sprints: 'Sprints',
    due_date: 'Due',
    created: 'Created',
    modified: 'Updated',
  }
  return labels[col]
}

function toggleColumn(col: ColumnKey, event: Event) {
  const checked = (event.target as HTMLInputElement).checked
  const next = new Set(columns.value)
  if (checked) next.add(col)
  else next.delete(col)
  next.add('id')
  next.add('title')
  columns.value = Array.from(next)
}

function resetColumns() {
  columns.value = [...defaultColumns]
}

const columnMenuOpen = ref(false)
const columnMenuRef = ref<HTMLElement | null>(null)
const columnMenuButtonRef = ref<HTMLElement | null>(null)

function toggleColumnMenu() {
  columnMenuOpen.value = !columnMenuOpen.value
}

function closeColumnMenu() {
  columnMenuOpen.value = false
}

function handleColumnMenuClick(event: MouseEvent) {
  if (!columnMenuOpen.value) return
  const target = event.target as Node | null
  if (!target) return
  if (columnMenuRef.value?.contains(target)) return
  if (columnMenuButtonRef.value?.contains(target)) return
  closeColumnMenu()
}

function handleColumnMenuKey(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    closeColumnMenu()
  }
}

const sortState = loadStoredSort()
const sort = reactive<{ key: ColumnKey | null; dir: 'asc' | 'desc' }>(
  sortState ?? {
    key: 'modified',
    dir: 'desc',
  },
)

watch(timeRange, (value) => {
  if (typeof window === 'undefined') return
  try {
    window.localStorage.setItem(timeRangeStorageKey, value)
  } catch {
    /* ignore persistence errors */
  }
})

watch(
  sort,
  (value) => {
    if (typeof window === 'undefined') return
    try {
      window.localStorage.setItem(sortStorageKey, JSON.stringify(value))
    } catch {
      /* ignore persistence errors */
    }
  },
  { deep: true },
)

function onSort(key: ColumnKey) {
  if (sort.key === key) {
    sort.dir = sort.dir === 'asc' ? 'desc' : 'asc'
  } else {
    sort.key = key
    sort.dir = 'asc'
  }
}

function ariaSort(key: ColumnKey) {
  if (sort.key !== key) return 'none'
  return sort.dir === 'asc' ? 'ascending' : 'descending'
}

function sortGlyph(key: ColumnKey) {
  if (sort.key !== key) return '⇅'
  return sort.dir === 'asc' ? '▲' : '▼'
}

const sortedSprints = computed(() => {
  const items = Array.isArray(sprints.value) ? [...sprints.value] : []
  const withStart = items.filter((item) => safeTimestamp(item.planned_start ?? null) !== null)
  const withoutStart = items.filter((item) => safeTimestamp(item.planned_start ?? null) === null)

  withStart.sort((a, b) => {
    const aStart = safeTimestamp(a.planned_start ?? null) ?? Number.MAX_SAFE_INTEGER
    const bStart = safeTimestamp(b.planned_start ?? null) ?? Number.MAX_SAFE_INTEGER
    return aStart - bStart
  })

  withoutStart.sort((a, b) => {
    const aCreated = safeTimestamp(a.created ?? null)
    const bCreated = safeTimestamp(b.created ?? null)
    if (aCreated !== null && bCreated !== null) return aCreated - bCreated
    if (aCreated !== null) return -1
    if (bCreated !== null) return 1
    return a.id - b.id
  })

  return [...withStart, ...withoutStart]
})

function sprintStateKey(sprint: SprintListItem | null | undefined): string {
  return (sprint?.state || '').toLowerCase()
}

function sprintIsFuture(sprint: SprintListItem) {
  return sprintStateKey(sprint) === 'pending'
}

const nonFutureSprints = computed(() => sortedSprints.value.filter((entry) => !sprintIsFuture(entry)))

const analyticsDefaultSprintId = computed<number | null>(() => {
  const items = sortedSprints.value
  if (!items.length) return null
  const active = [...items]
    .reverse()
    .find((entry) => {
      const state = sprintStateKey(entry)
      return state === 'active' || state === 'overdue'
    })
  if (active) return active.id
  const fallbackPool = nonFutureSprints.value
  if (fallbackPool.length) {
    return fallbackPool[fallbackPool.length - 1]?.id ?? null
  }
  return items[items.length - 1]?.id ?? null
})

const showCurrentOnly = computed(() => timeRange.value === 'current')

const timeRangeDays = computed(() => {
  if (timeRange.value === 'all' || timeRange.value === 'current') {
    return null
  }
  return Number(timeRange.value)
})

function sprintIsComplete(sprint: SprintListItem) {
  return sprintStateKey(sprint) === 'complete'
}

function sprintReferenceTimestamp(sprint: SprintListItem): number | null {
  return (
    safeTimestamp(sprint.actual_end ?? null) ??
    safeTimestamp(sprint.planned_end ?? null) ??
    safeTimestamp(sprint.computed_end ?? null) ??
    safeTimestamp(sprint.modified ?? null) ??
    safeTimestamp(sprint.created ?? null)
  )
}

const visibleSprints = computed(() => {
  if (showCurrentOnly.value) {
    return sortedSprints.value.filter((item) => !sprintIsComplete(item))
  }

  const days = timeRangeDays.value
  if (days === null) {
    return sortedSprints.value
  }
  const cutoff = Date.now() - days * 24 * 60 * 60 * 1000
  return sortedSprints.value.filter((item) => {
    if (!sprintIsComplete(item)) {
      return true
    }
    const timestamp = sprintReferenceTimestamp(item)
    if (timestamp === null) {
      return true
    }
    return timestamp >= cutoff
  })
})

const hiddenClosedCount = computed(() => {
  const totalClosed = sortedSprints.value.filter((item) => sprintIsComplete(item)).length
  const visibleClosed = visibleSprints.value.filter((item) => sprintIsComplete(item)).length
  return Math.max(0, totalClosed - visibleClosed)
})

const showAllowClosedControl = computed(
  () => !showCurrentOnly.value && visibleSprints.value.some((item) => sprintIsComplete(item)),
)

const sprintLookup = computed(() => {
  const map: Record<number, { label: string; state?: string }> = {}
  sortedSprints.value.forEach((entry) => {
    map[entry.id] = { label: entry.display_name, state: entry.state }
  })
  return map
})

function isClosedSprintId(id: number | null | undefined) {
  if (!id) return false
  const state = sprintLookup.value[id]?.state || ''
  return state.toLowerCase() === 'complete'
}

function sprintLabelById(id: number | null | undefined) {
  if (!id) return 'this sprint'
  return sprintLookup.value[id]?.label || `Sprint #${id}`
}

watch(showAllowClosedControl, (value) => {
  if (!value) {
    allowClosed.value = false
  }
})

watch(
  sortedSprints,
  (list) => {
    if (!Array.isArray(list) || list.length === 0) {
      selectedAnalyticsSprintId.value = null
      return
    }
    const current = selectedAnalyticsSprintId.value
    const exists = current ? list.some((item) => item.id === current) : false
    if (!current || !exists) {
      selectedAnalyticsSprintId.value = analyticsDefaultSprintId.value ?? list[list.length - 1].id
    }
  },
  { immediate: true },
)

watch(
  () => selectedAnalyticsSprintId.value,
  (id) => {
    if (!id || analyticsModal.open) return
    void ensureSprintAnalytics()
  },
  { immediate: true },
)

watch(
  () => [selectedAnalyticsSprintId.value, velocityHistoryLimit.value],
  ([id]) => {
    if (!id || !analyticsModal.open) return
    void ensureVelocity(false)
  },
)

const missingSprintMessage = computed(() => {
  if (!hasMissingSprints.value) return ''
  return missingSprints.value.map((id) => `#${id}`).join(', ')
})

const multiSprintTaskIds = computed(() => {
  const set = new Set<string>()
  tasks.value.forEach((task) => {
    if (Array.isArray(task.sprints) && task.sprints.length > 1) {
      set.add(task.id)
    }
  })
  return set
})

function multiSprintTooltip(task: TaskDTO, currentSprintId: number | null): string {
  const memberships = Array.isArray(task.sprints)
    ? task.sprints.filter((id): id is number => typeof id === 'number' && Number.isFinite(id))
    : []
  const uniqueMemberships = Array.from(new Set(memberships))
  if (uniqueMemberships.length <= 1) {
    return 'Assigned to multiple sprints'
  }
  const labelFor = (id: number) => sprintLookup.value[id]?.label || `Sprint #${id}`
  const otherIds = currentSprintId === null
    ? uniqueMemberships
    : uniqueMemberships.filter((id) => id !== currentSprintId)
  const hints = Array.from(new Set(otherIds.map(labelFor).filter((label) => label && label.trim().length > 0)))
  if (hints.length) {
    return `Also assigned to ${hints.join(', ')}`
  }
  const fallback = Array.from(new Set(uniqueMemberships.map(labelFor)))
  if (fallback.length) {
    return `Also assigned to ${fallback.join(', ')}`
  }
  return 'Assigned to multiple sprints'
}

const tasksBySprint = computed(() => {
  const map = new Map<number, TaskDTO[]>()
  tasks.value.forEach((task) => {
    const memberships = Array.isArray(task.sprints) ? task.sprints : []
    memberships.forEach((id) => {
      if (!map.has(id)) {
        map.set(id, [])
      }
      map.get(id)!.push(task)
    })
  })
  return map
})

function capacitySummary(sprint: SprintListItem): string {
  const points = typeof sprint.capacity_points === 'number' ? sprint.capacity_points : null
  const hours = typeof sprint.capacity_hours === 'number' ? sprint.capacity_hours : null
  if (points === null && hours === null) {
    return ''
  }
  if (points !== null && hours !== null) {
    return `${points} pts / ${hours} h`
  }
  if (points !== null) {
    return `${points} pts`
  }
  return `${hours} h`
}

const backlogTasks = computed(() => tasks.value.filter((task) => !(Array.isArray(task.sprints) && task.sprints.length > 0)))

const filtersActive = computed(() => Object.entries(filter.value).some(([key, value]) => key !== 'order' && !!value))

const busy = computed(
  () => sprintsLoading.value || tasksLoading.value || Object.keys(lifecycleBusy).length > 0,
)

const sortedBacklogTasks = computed(() => sortTasks(backlogTasks.value))

const analyticsSummary = computed(
  () => sprintAnalytics.getSummary(selectedAnalyticsSprintId.value) as SprintSummaryReportResponse | undefined,
)
const analyticsBurndown = computed(
  () => sprintAnalytics.getBurndown(selectedAnalyticsSprintId.value) as SprintBurndownResponse | undefined,
)
const summaryLoading = computed(() => sprintAnalytics.isSummaryLoading(selectedAnalyticsSprintId.value))
const summaryError = computed(() => sprintAnalytics.getSummaryError(selectedAnalyticsSprintId.value))
const burndownLoading = computed(() => sprintAnalytics.isBurndownLoading(selectedAnalyticsSprintId.value))
const burndownError = computed(() => sprintAnalytics.getBurndownError(selectedAnalyticsSprintId.value))
const analyticsLoading = computed(() => summaryLoading.value || burndownLoading.value)
const analyticsError = computed(() => summaryError.value ?? burndownError.value)

const velocityFocusSprintIds = computed(() => {
  const items = nonFutureSprints.value
  if (!items.length) return []
  const selectedId = selectedAnalyticsSprintId.value
  let anchorIndex = selectedId ? items.findIndex((entry) => entry.id === selectedId) : -1
  if (anchorIndex === -1) {
    anchorIndex = items.length - 1
  }
  const start = Math.max(0, anchorIndex - 3)
  return items.slice(start, anchorIndex + 1).map((entry) => entry.id)
})

const velocityWindowSize = computed(() => velocityFocusSprintIds.value.length || 0)

const velocityHistoryLimit = computed(() => {
  const items = nonFutureSprints.value
  if (!items.length) return VELOCITY_WINDOW
  const selectedId = selectedAnalyticsSprintId.value
  let index = selectedId ? items.findIndex((entry) => entry.id === selectedId) : -1
  if (index === -1) {
    index = items.length - 1
  }
  const remaining = items.length - index
  return Math.max(VELOCITY_WINDOW, remaining)
})

const velocityParams = computed(() => ({
  limit: velocityHistoryLimit.value,
  include_active: DEFAULT_VELOCITY_PARAMS.include_active,
  metric: analyticsModal.velocityMetric,
}))

const analyticsVelocity = computed(
  () => sprintAnalytics.getVelocity(velocityParams.value) as SprintVelocityResponse | undefined,
)
const velocityLoading = computed(() => sprintAnalytics.isVelocityLoading(velocityParams.value))
const velocityError = computed(() => sprintAnalytics.getVelocityError(velocityParams.value))

watch(visibleSprints, (list) => {
  const keep = new Set(list.map((sprint) => sectionKey(sprint.id)))
  Object.keys(expanded)
    .filter((key) => key.startsWith('sprint-') && !keep.has(key))
    .forEach((key) => {
      delete expanded[key]
    })
  list.forEach((sprint) => {
    if (!(sectionKey(sprint.id) in expanded)) {
      expanded[sectionKey(sprint.id)] = true
    }
  })
})

watch(
  filter,
  () => {
    scheduleTasksRefresh()
  },
  { deep: true },
)

watch(
  () => filter.value.project || '',
  (project) => {
    refreshConfigDefaults(project || undefined).catch((error) => {
      console.warn('Failed to refresh sprint defaults', error)
    })
  },
  { immediate: true },
)

watch(
  () => modal.open,
  (open) => {
    if (open) {
      nextTick(() => {
        firstField.value?.focus()
      })
    }
  },
)

watch(
  () => modal.open || deleteDialog.open,
  (open) => {
    if (open) {
      addModalListeners()
    } else {
      removeModalListeners()
    }
  },
)

watch(
  () => analyticsModal.open,
  (open) => {
    if (open) {
      if (!selectedAnalyticsSprintId.value && analyticsDefaultSprintId.value) {
        selectedAnalyticsSprintId.value = analyticsDefaultSprintId.value
      }
      if (selectedAnalyticsSprintId.value) {
        void ensureSprintAnalytics(true)
      }
      void ensureVelocity(true)
    }
  },
)

watch(
  () => selectedAnalyticsSprintId.value,
  (id, prev) => {
    if (!analyticsModal.open) return
    if (id && id !== prev) {
      void ensureSprintAnalytics(true)
    }
  },
)

watch(
  () => analyticsModal.velocityMetric,
  () => {
    if (!analyticsModal.open) return
    void ensureVelocity(false)
  },
)

watch(
  () => route.hash,
  () => {
    focusBacklogIfRequested()
  },
)

watch(
  () => route.query.backlog,
  () => {
    focusBacklogIfRequested()
  },
)

let updatingLengthFromDates = false
let updatingEndFromLength = false

watch(
  () => [form.starts_at, form.ends_at] as const,
  ([startValue, endValue]) => {
    if (updatingEndFromLength) return
    const startTs = dateTimeInputToTimestamp(startValue)
    const endTs = dateTimeInputToTimestamp(endValue)
    if (startTs === null || endTs === null) {
      return
    }
    if (endTs <= startTs) {
      if (!updatingLengthFromDates && form.plan_length) {
        updatingLengthFromDates = true
        try {
          form.plan_length = ''
        } finally {
          updatingLengthFromDates = false
        }
      }
      return
    }
    const formatted = formatDurationForPlanLength(endTs - startTs)
    if (!formatted || form.plan_length === formatted) {
      return
    }
    updatingLengthFromDates = true
    try {
      form.plan_length = formatted
    } finally {
      updatingLengthFromDates = false
    }
  },
  { immediate: true },
)

watch(
  () => form.plan_length,
  (value) => {
    if (updatingLengthFromDates) return
    const startTs = dateTimeInputToTimestamp(form.starts_at)
    if (startTs === null) {
      return
    }
    const durationMs = parseDurationToMs(value)
    if (durationMs === null || durationMs <= 0) {
      return
    }
    const newEndInput = timestampToDateTimeInput(startTs + durationMs)
    if (!newEndInput || form.ends_at === newEndInput) {
      return
    }
    updatingEndFromLength = true
    try {
      form.ends_at = newEndInput
    } finally {
      updatingEndFromLength = false
    }
  },
)

watch(
  () => form.starts_at,
  (value) => {
    if (!value || form.ends_at || updatingEndFromLength) {
      return
    }
    const durationMs = parseDurationToMs(form.plan_length)
    if (durationMs === null || durationMs <= 0) {
      return
    }
    const startTs = dateTimeInputToTimestamp(value)
    if (startTs === null) {
      return
    }
    const newEndInput = timestampToDateTimeInput(startTs + durationMs)
    if (!newEndInput) {
      return
    }
    updatingEndFromLength = true
    try {
      form.ends_at = newEndInput
    } finally {
      updatingEndFromLength = false
    }
  },
)

function clearFilters() {
  filter.value = {}
  filterBarRef.value?.clear?.()
}

function toStringValue(value: unknown) {
  if (typeof value === 'string') return value
  if (value === null || value === undefined) return ''
  try {
    return String(value)
  } catch {
    return ''
  }
}

function toTrimmedString(value: unknown) {
  return toStringValue(value).trim()
}

const DURATION_UNIT_MS: Record<string, number> = {
  ms: 1,
  millisecond: 1,
  milliseconds: 1,
  s: 1_000,
  sec: 1_000,
  secs: 1_000,
  second: 1_000,
  seconds: 1_000,
  m: 60_000,
  min: 60_000,
  mins: 60_000,
  minute: 60_000,
  minutes: 60_000,
  h: 3_600_000,
  hr: 3_600_000,
  hrs: 3_600_000,
  hour: 3_600_000,
  hours: 3_600_000,
  d: 86_400_000,
  day: 86_400_000,
  days: 86_400_000,
  w: 604_800_000,
  week: 604_800_000,
  weeks: 604_800_000,
}

const PLAN_LENGTH_UNITS = [
  { label: 'w', size: 604_800_000 },
  { label: 'd', size: 86_400_000 },
  { label: 'h', size: 3_600_000 },
  { label: 'm', size: 60_000 },
]

function parseDurationToMs(value: string | null | undefined): number | null {
  if (!value) return null
  const text = value.trim().toLowerCase()
  if (!text) return null
  const pattern = /([0-9]*\.?[0-9]+)\s*([a-z]+)/g
  let total = 0
  let matched = false
  let match: RegExpExecArray | null
  while ((match = pattern.exec(text)) !== null) {
    matched = true
    const amount = Number.parseFloat(match[1])
    if (!Number.isFinite(amount)) {
      continue
    }
    const unitRaw = match[2]
    const unitMs = DURATION_UNIT_MS[unitRaw] ?? DURATION_UNIT_MS[unitRaw.replace(/s$/, '')]
    if (!unitMs) {
      continue
    }
    total += amount * unitMs
  }
  if (!matched) {
    return null
  }
  return total > 0 ? total : null
}

function dateTimeInputToTimestamp(value: string | null | undefined): number | null {
  if (!value) return null
  const normalized = fromDateTimeInputValue(value)
  if (!normalized) return null
  const timestamp = safeTimestamp(normalized)
  return timestamp
}

function timestampToDateTimeInput(timestamp: number): string {
  if (!Number.isFinite(timestamp)) return ''
  const rounded = Math.round(timestamp / 60_000) * 60_000
  return toDateTimeInputValue(new Date(rounded).toISOString())
}

function formatDurationForPlanLength(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return ''
  }
  let remaining = ms
  const parts: string[] = []
  for (const unit of PLAN_LENGTH_UNITS) {
    const value = Math.floor(remaining / unit.size)
    if (value > 0) {
      parts.push(`${value}${unit.label}`)
      remaining -= value * unit.size
    }
  }
  if (!parts.length && remaining > 0) {
    return '1m'
  }
  return parts.join(' ')
}

function formatElapsedDuration(ms: number): string {
  if (!Number.isFinite(ms) || ms <= 0) {
    return '<1m'
  }
  const units = [
    { label: 'd', size: 86_400_000 },
    { label: 'h', size: 3_600_000 },
    { label: 'm', size: 60_000 },
  ]
  const parts: string[] = []
  let remaining = ms
  for (const unit of units) {
    if (remaining < unit.size && parts.length === 0) {
      continue
    }
    if (remaining < unit.size) {
      continue
    }
    const value = Math.floor(remaining / unit.size)
    if (value > 0) {
      parts.push(`${value}${unit.label}`)
      remaining -= value * unit.size
    }
    if (parts.length === 2) {
      break
    }
  }
  if (!parts.length) {
    return '<1m'
  }
  return parts.join(' ')
}

interface StartDelayInfo {
  elapsed: string
  severity: 'warning' | 'danger'
  limitLabel?: string
}

const overdueLimitLookup = computed(() => {
  const fallback = toTrimmedString(sprintDefaults.value?.overdue_after ?? '')
  const map = new Map<number, string>()
  sprints.value.forEach((sprint) => {
    const specific = toTrimmedString(sprint.overdue_after ?? '')
    if (specific) {
      map.set(sprint.id, specific)
    } else if (fallback) {
      map.set(sprint.id, fallback)
    }
  })
  return map
})

const startDelayLookup = computed(() => {
  const fallbackLabel = toTrimmedString(sprintDefaults.value?.overdue_after ?? '')
  const now = Date.now()
  const map = new Map<number, StartDelayInfo>()
  sprints.value.forEach((sprint) => {
    if (sprint.actual_start) return
    const plannedTs = safeTimestamp(sprint.planned_start ?? null)
    if (plannedTs === null) return
    if (now <= plannedTs) return
    const specific = toTrimmedString(sprint.overdue_after ?? '')
    const limitLabel = specific || fallbackLabel
    const limitMs = parseDurationToMs(limitLabel)
    const elapsedMs = now - plannedTs
    const severity: 'warning' | 'danger' = limitMs !== null && elapsedMs > limitMs ? 'danger' : 'warning'
    map.set(sprint.id, {
      elapsed: formatElapsedDuration(elapsedMs),
      severity,
      limitLabel: limitLabel || undefined,
    })
  })
  return map
})

function parseList(value: string): string[] {
  return value
    .split(',')
    .map((token) => token.trim())
    .filter(Boolean)
}

function applySprintSmartFilters(source: TaskDTO[], q: Record<string, string>): TaskDTO[] {
  const wantsUnassigned = q.assignee === '__none__'
  const due = q.due || ''
  const recent = q.recent || ''
  const needsSet = new Set((q.needs || '').split(',').map((s) => s.trim()).filter(Boolean))
  const now = new Date()
  const today = startOfDay(now)
  const tomorrow = new Date(today.getTime() + MS_PER_DAY)
  const soonCutoff = new Date(today.getTime() + 7 * MS_PER_DAY)
  const recentCutoff = new Date(now.getTime() - 7 * MS_PER_DAY)

  return source.filter((task) => {
    if (wantsUnassigned && (task.assignee || '').trim()) {
      return false
    }

    if (due) {
      const dueDate = parseDateLike(task.due_date)
      if (!dueDate) {
        return false
      }
      const dueTime = startOfDay(dueDate).getTime()
      const todayStart = today.getTime()
      const tomorrowStart = tomorrow.getTime()
      const soonCutoffTime = startOfDay(soonCutoff).getTime()
      if (due === 'today') {
        if (dueTime < todayStart || dueTime >= tomorrowStart) {
          return false
        }
      } else if (due === 'soon') {
        if (dueTime < tomorrowStart || dueTime > soonCutoffTime) {
          return false
        }
      } else if (due === 'later') {
        if (dueTime <= soonCutoffTime) {
          return false
        }
      } else if (due === 'overdue') {
        if (dueTime >= todayStart) {
          return false
        }
      }
    }

    if (recent === '7d') {
      const modified = parseDateLike(task.modified)
      if (!modified || modified.getTime() < recentCutoff.getTime()) {
        return false
      }
    }

    if (needsSet.size) {
      if (needsSet.has('effort')) {
        const effort = (task.effort || '').trim()
        if (effort) {
          return false
        }
      }
      if (needsSet.has('due')) {
        if ((task.due_date || '').trim()) {
          return false
        }
      }
    }

    return true
  })
}

function sectionKey(id: number) {
  return `sprint-${id}`
}

function isExpanded(key: string) {
  if (!(key in expanded)) {
    expanded[key] = true
  }
  return expanded[key]
}

function toggleSection(key: string) {
  expanded[key] = !isExpanded(key)
}

function scheduleTasksRefresh() {
  if (typeof window === 'undefined') {
    void refreshTasks()
    return
  }
  if (filterTimer.value !== null) {
    window.clearTimeout(filterTimer.value)
  }
  filterTimer.value = window.setTimeout(() => {
    filterTimer.value = null
    void refreshTasks()
  }, 300)
}

async function refreshTasks() {
  if (typeof window !== 'undefined' && filterTimer.value !== null) {
    window.clearTimeout(filterTimer.value)
    filterTimer.value = null
  }
  tasksLoading.value = true
  try {
    const rawFilter = { ...(filter.value || {}) }
    const qnorm: Record<string, string> = {}
    const extraQuery: Record<string, string> = {}
    for (const [key, value] of Object.entries(rawFilter)) {
      if (!value || key === 'order') continue
      if (BUILTIN_QUERY_KEYS.has(key)) {
        qnorm[key] = value
      } else {
        extraQuery[key] = value
      }
    }

    const payload: Record<string, unknown> = {}
    if (qnorm.q) payload.q = qnorm.q
    if (qnorm.project) payload.project = qnorm.project
    const statusList = parseList(qnorm.status || '')
    if (statusList.length) payload.status = statusList
    const priorityList = parseList(qnorm.priority || '')
    if (priorityList.length) payload.priority = priorityList
    const typeList = parseList(qnorm.type || '')
    if (typeList.length) payload.type = typeList
    const tagList = parseList(qnorm.tags || '')
    if (tagList.length) payload.tags = tagList
    if (qnorm.assignee && qnorm.assignee !== '__none__') payload.assignee = qnorm.assignee
    Object.entries(extraQuery).forEach(([key, value]) => {
      payload[key] = value
    })

    const response = (await api.listTasks(payload as any)) as TaskDTO[]
    const normalized: TaskDTO[] = Array.isArray(response)
      ? response.map((task) => normalizeTaskRecord(task))
      : []
    const filtered = applySprintSmartFilters(normalized, qnorm)
    tasks.value = filtered
  } catch (error: any) {
    showToast(error?.message || 'Failed to load sprint tasks')
    tasks.value = []
  } finally {
    tasksLoading.value = false
    if (!initialized.value) {
      initialized.value = true
    }
  }
}

async function refreshAll(force = false) {
  try {
    await refreshSprints(force)
  } catch (error: any) {
    showToast(error?.message || 'Failed to load sprints')
  }
  await refreshTasks()
}

async function handleManualRefresh() {
  await refreshAll(true)
}

function notifyIntegrity(integrity?: import('../api/types').SprintIntegrityDiagnostics | null) {
  if (!integrity) return
  const removed = integrity.auto_cleanup?.removed_references ?? 0
  if (removed) {
    showToast(`Automatically cleaned ${removed} dangling sprint reference${removed === 1 ? '' : 's'}.`)
  }
  const missing = integrity.missing_sprints ?? []
  if (missing.length) {
    showToast(`Missing sprint IDs still detected: ${missing.map((id) => `#${id}`).join(', ')}`)
  }
}

interface MutationOptions {
  silent?: boolean
  forceSingle?: boolean
}

async function assignTasksToSprint(taskIds: string[], sprintId: number, options: MutationOptions = {}): Promise<boolean> {

  if (!taskIds.length) return false
  const targetMeta = sprintLookup.value[sprintId]
  const sprintState = (targetMeta?.state || '').toLowerCase()
  const isClosedTarget = sprintState === 'complete'
  const sprintName = targetMeta?.label || `Sprint #${sprintId}`
  if (isClosedTarget && !allowClosed.value) {
    if (!options.silent) {
      showToast(`Enable "Allow editing closed sprints" above to modify ${sprintName}.`)
    }
    return false
  }

  const forceSingle = options.forceSingle ?? !copyModifierActive.value
  const payload: Record<string, unknown> = {
    tasks: [...taskIds],
    cleanup_missing: true,
    sprint: sprintId,
  }
  if (forceSingle) {
    payload.force_single = true
  }
  if (allowClosed.value) payload.allow_closed = true

  try {
    const response = await api.sprintAdd(payload as any)
    const changed = response.modified.length
    const replaced = Array.isArray(response.replaced) && response.replaced.length > 0
    const responseName = response.sprint_label || (response.sprint_id ? `Sprint #${response.sprint_id}` : 'sprint')

    if (!options.silent) {
      if (changed > 0) {
        showToast(`Added ${changed} task${changed === 1 ? '' : 's'} to ${responseName}`)
      } else if (!replaced) {
        showToast(`No changes were applied for ${responseName}`)
      }

      const messages = Array.isArray(response.messages) ? response.messages : []
      if (messages.length) {
        messages.forEach((message: string) => showToast(message))
      } else if (replaced && Array.isArray(response.replaced)) {
        response.replaced.forEach((entry: any) => {
          if (!entry?.previous?.length) return
          const prior = entry.previous.map((id: number | string) => `#${id}`).join(', ')
          showToast(`${entry.task_id} moved from ${prior}`)
        })
      }
    }

    if (!options.silent) {
      notifyIntegrity(response.integrity)
    }
    return changed > 0 || replaced
  } catch (error: any) {
    showToast(error?.message || 'Failed to assign to sprint')
    throw error
  }
}

async function removeTasksFromSprint(taskIds: string[], sprintId: number, options: MutationOptions = {}): Promise<boolean> {
  if (!taskIds.length) return false
  const targetMeta = sprintLookup.value[sprintId]
  const sprintState = (targetMeta?.state || '').toLowerCase()
  const isClosedTarget = sprintState === 'complete'
  const sprintName = targetMeta?.label || `Sprint #${sprintId}`
  if (isClosedTarget && !allowClosed.value) {
    if (!options.silent) {
      showToast(`Enable "Allow editing closed sprints" above to modify ${sprintName}.`)
    }
    return false
  }

  const payload: Record<string, unknown> = {
    tasks: [...taskIds],
    cleanup_missing: true,
    sprint: sprintId,
  }

  try {
    const response = await api.sprintRemove(payload as any)
    const changed = response.modified.length
    const responseName = response.sprint_label || (response.sprint_id ? `Sprint #${response.sprint_id}` : 'sprint')
    if (changed > 0 && !options.silent) {
      showToast(`Removed ${changed} task${changed === 1 ? '' : 's'} from ${responseName}`)
    }
    if (!options.silent) {
      const messages = Array.isArray(response.messages) ? response.messages : []
      messages.forEach((message: string) => showToast(message))
    }
    if (!options.silent) {
      notifyIntegrity(response.integrity)
    }
    return changed > 0
  } catch (error: any) {
    showToast(error?.message || 'Failed to remove from sprint')
    throw error
  }
}

function onTaskDragStart(event: DragEvent, taskId: string, sourceSprint: number | null) {
  const target = event.target as HTMLElement | null
  if (target && target.closest('button, a, input, textarea, select')) {
    event.preventDefault()
    return
  }
  resolveCopyModifier(event)
  draggingTaskId.value = taskId
  draggingSourceSprint.value = sourceSprint
  event.dataTransfer?.setData('text/plain', taskId)
  if (event.dataTransfer) {
    event.dataTransfer.effectAllowed = 'copyMove'
  }
}

function onTaskDragEnd() {
  draggingTaskId.value = null
  draggingSourceSprint.value = null
  hoverSprintId.value = null
  hoverBacklog.value = false
  resetCopyModifier()
}

function onSprintDragEnter(_event: DragEvent, id: number) {
  if (!draggingTaskId.value) return
  hoverSprintId.value = id
}

function onSprintDragLeave(id: number) {
  if (hoverSprintId.value === id) {
    hoverSprintId.value = null
  }
}

function onSprintDragOver(event: DragEvent) {
  if (!draggingTaskId.value) return
  event.preventDefault()
  if (event.dataTransfer) {
    const copyMode = resolveCopyModifier(event)
    event.dataTransfer.dropEffect = copyMode ? 'copy' : 'move'
  }
}

async function onSprintDrop(event: DragEvent, id: number) {
  if (!draggingTaskId.value) return
  resolveCopyModifier(event)
  const taskId = draggingTaskId.value
  const source = draggingSourceSprint.value
  const copyMode = resolveCopyModifier(event)
  hoverSprintId.value = null
  if (!allowClosed.value) {
    if (isClosedSprintId(id)) {
      showToast(`Enable "Allow editing closed sprints" above to modify ${sprintLabelById(id)}.`)
      onTaskDragEnd()
      return
    }
    if (source && isClosedSprintId(source)) {
      showToast(`Enable "Allow editing closed sprints" above to modify ${sprintLabelById(source)}.`)
      onTaskDragEnd()
      return
    }
  }
  let added = false
  try {
    added = await assignTasksToSprint([taskId], id, { forceSingle: !copyMode })
    if (added && !copyMode && source && source !== id) {
      await removeTasksFromSprint([taskId], source, { silent: true })
    }
  } catch {
    /* errors already surfaced */
  } finally {
    if (added) {
      await refreshAll(true)
    }
    onTaskDragEnd()
  }
}

function onBacklogDragEnter(event: DragEvent) {
  if (!draggingTaskId.value) return
  const copyMode = resolveCopyModifier(event)
  hoverBacklog.value = !copyMode
  if (event.dataTransfer) {
    event.dataTransfer.dropEffect = copyMode ? 'none' : 'move'
  }
}

function onBacklogDragLeave() {
  hoverBacklog.value = false
}

function onBacklogDragOver(event: DragEvent) {
  if (!draggingTaskId.value) return
  event.preventDefault()
  if (event.dataTransfer) {
    const copyMode = resolveCopyModifier(event)
    hoverBacklog.value = !copyMode
    event.dataTransfer.dropEffect = copyMode ? 'none' : 'move'
  }
}

async function onBacklogDrop(event: DragEvent) {
  if (!draggingTaskId.value) return
  resolveCopyModifier(event)
  const taskId = draggingTaskId.value
  const copyMode = resolveCopyModifier(event)
  hoverBacklog.value = false
  if (copyMode) {
    showToast('Release the copy modifier to move a task back to the backlog.')
    onTaskDragEnd()
    return
  }
  const task = tasks.value.find((item) => item.id === taskId)
  const memberships = Array.isArray(task?.sprints)
    ? Array.from(
        new Set(
          task!.sprints.filter((id): id is number => typeof id === 'number' && Number.isFinite(id) && id > 0),
        ),
      )
    : []
  if (!memberships.length) {
    onTaskDragEnd()
    return
  }
  if (!allowClosed.value) {
    const blockedSprint = memberships.find((id) => isClosedSprintId(id))
    if (blockedSprint) {
      showToast(`Enable "Allow editing closed sprints" above to modify ${sprintLabelById(blockedSprint)}.`)
      onTaskDragEnd()
      return
    }
  }
  let removedAny = false
  try {
    for (let index = 0; index < memberships.length; index += 1) {
      const sprintId = memberships[index]
      const result = await removeTasksFromSprint([taskId], sprintId, { silent: index > 0 })
      removedAny = removedAny || result
    }
  } catch {
    /* toast already shown */
  } finally {
    if (removedAny) {
      await refreshAll(true)
    }
    onTaskDragEnd()
  }
}

function handleTaskPanelUpdated(updated: TaskDTO) {
  if (!updated || !updated.id) return
  const normalized = normalizeTaskRecord(updated)
  const index = tasks.value.findIndex((item) => item.id === normalized.id)
  const previous = index >= 0 ? tasks.value[index] : null
  const membershipUnchanged = previous ? sprintMembershipMatches(previous, normalized) : false
  if (index >= 0) {
    tasks.value[index] = normalized
  } else {
    tasks.value.push(normalized)
  }
  if (index < 0 || !membershipUnchanged) {
    void refreshAll(true)
  }
}

function onTaskRowClick(task: TaskDTO, event: MouseEvent) {
  if (!task?.id) return
  if (event?.defaultPrevented) return
  if (typeof event?.button === 'number' && event.button !== 0) return
  if (draggingTaskId.value) return
  const target = event?.target as HTMLElement | null
  if (target && target.closest('button, a, input, textarea, select')) {
    return
  }
  const prefix = projectOf(task.id)
  openTaskPanel({
    taskId: task.id,
    initialProject: prefix ? prefix : null,
    onUpdated: handleTaskPanelUpdated,
  })
  event?.stopPropagation()
}

function getTasksForSprint(id: number) {
  return tasksBySprint.value.get(id) ?? []
}

function countExclusiveSprintTasks(list: TaskDTO[] | undefined | null, sprintId: number) {
  if (!Array.isArray(list) || !list.length) return 0
  let total = 0
  for (const task of list) {
    const memberships = normalizeSprintMembership((task as any).sprints)
    if (memberships.length === 1 && memberships[0] === sprintId) {
      total += 1
    }
  }
  return total
}

function sortTasks(list: TaskDTO[] | undefined | null) {
  const arr = Array.isArray(list) ? [...list] : []
  if (!sort.key) return arr
  const key = sort.key
  const dir = sort.dir === 'asc' ? 1 : -1
  arr.sort((a, b) => {
    const av = (a as any)[key]
    const bv = (b as any)[key]
    if (av == null && bv == null) return 0
    if (av == null) return -1 * dir
    if (bv == null) return 1 * dir
    if (key === 'sprints') {
      const as = Array.isArray(av) && av.length ? av.join(',') : ''
      const bs = Array.isArray(bv) && bv.length ? bv.join(',') : ''
      return as.localeCompare(bs) * dir
    }
    if (key === 'tags') {
      const as = Array.isArray(av) ? av.join(',') : String(av ?? '')
      const bs = Array.isArray(bv) ? bv.join(',') : String(bv ?? '')
      return as.localeCompare(bs) * dir
    }
    if (key === 'due_date' || key === 'created' || key === 'modified') {
      const at = safeTimestamp(String(av)) ?? 0
      const bt = safeTimestamp(String(bv)) ?? 0
      return (at - bt) * dir
    }
    return String(av).localeCompare(String(bv)) * dir
  })
  return arr
}

function getSortedTasksForSprint(id: number) {
  return sortTasks(getTasksForSprint(id))
}

function parseNumberInput(value: unknown): number | null {
  const trimmed = toTrimmedString(value)
  if (!trimmed) return null
  const parsed = Number(trimmed)
  return Number.isFinite(parsed) ? parsed : null
}

function formatDue(value?: string | null) {
  if (!value) return ''
  try {
    const date = new Date(value)
    if (Number.isNaN(date.getTime())) return value
    return date.toLocaleDateString()
  } catch {
    return value
  }
}

function formatShortDate(value?: string | null) {
  if (!value) return ''
  try {
    return new Date(value).toLocaleDateString(undefined, { dateStyle: 'medium' })
  } catch {
    return value || ''
  }
}

function badgeClass(state: string) {
  const lowered = (state || '').toLowerCase()
  if (lowered === 'active') return 'badge--info'
  if (lowered === 'overdue') return 'badge--danger'
  if (lowered === 'complete') return 'badge--success'
  return 'badge--muted'
}

function projectOf(id: string) {
  return (id || '').split('-')[0]
}

function numericOf(id: string) {
  return (id || '').split('-').slice(1).join('-')
}

const relativeTimeFormatter =
  typeof Intl !== 'undefined' && (Intl as any).RelativeTimeFormat
    ? new Intl.RelativeTimeFormat(undefined, { numeric: 'auto' })
    : null

const relativeUnits: Array<{ unit: Intl.RelativeTimeFormatUnit; ms: number }> = [
  { unit: 'year', ms: 1000 * 60 * 60 * 24 * 365 },
  { unit: 'month', ms: 1000 * 60 * 60 * 24 * 30 },
  { unit: 'week', ms: 1000 * 60 * 60 * 24 * 7 },
  { unit: 'day', ms: 1000 * 60 * 60 * 24 },
  { unit: 'hour', ms: 1000 * 60 * 60 },
  { unit: 'minute', ms: 1000 * 60 },
  { unit: 'second', ms: 1000 },
]

function formatRelative(value?: string | null) {
  if (!value) return '—'
  const timestamp = safeTimestamp(value)
  if (timestamp === null) return value
  if (!relativeTimeFormatter) return new Date(timestamp).toLocaleString()
  const diff = timestamp - Date.now()
  for (const { unit, ms } of relativeUnits) {
    if (Math.abs(diff) >= ms || unit === 'second') {
      const amount = Math.round(diff / ms)
      return relativeTimeFormatter.format(amount, unit)
    }
  }
  return new Date(timestamp).toLocaleString()
}

function formatExact(value?: string | null) {
  if (!value) return ''
  try {
    return new Date(value).toLocaleString()
  } catch {
    return value || ''
  }
}

function isTaskOverdue(task: TaskDTO) {
  try {
    const status = (task.status || '').toLowerCase()
    if (!task.due_date || status === 'done') return false
    const due = safeTimestamp(task.due_date)
    if (due === null) return false
    const today = new Date()
    today.setHours(0, 0, 0, 0)
    return due < today.getTime()
  } catch {
    return false
  }
}

function sprintLabel(id: number) {
  const entry = sprintLookup.value[id]
  if (entry?.label) return entry.label
  return `#${id}`
}

function sprintStateClass(id: number) {
  const state = sprintLookup.value[id]?.state?.toLowerCase()
  if (!state) return 'sprint--unknown'
  return `sprint--${state}`
}

function addModalListeners() {
  if (typeof window === 'undefined') return
  window.addEventListener('keydown', handleKeydown)
}

function removeModalListeners() {
  if (typeof window === 'undefined') return
  window.removeEventListener('keydown', handleKeydown)
}

function resetDeleteDialog() {
  deleteDialog.sprintId = null
  deleteDialog.label = ''
  deleteDialog.backlogCount = 0
}

function openDelete(sprint: SprintListItem) {
  if (deleteDialogSubmitting.value) return
  deleteDialog.sprintId = sprint.id
  deleteDialog.label = (sprint.display_name || sprint.label || `Sprint #${sprint.id}`).trim()
  const sprintTasks = getTasksForSprint(sprint.id)
  deleteDialog.backlogCount = countExclusiveSprintTasks(sprintTasks, sprint.id)
  deleteDialog.open = true
}

function closeDeleteDialog(forceOrEvent?: boolean | Event) {
  const force = typeof forceOrEvent === 'boolean' ? forceOrEvent : false
  if (!force && deleteDialogSubmitting.value) return
  deleteDialog.open = false
  resetDeleteDialog()
}

async function confirmDeleteSprint() {
  if (!deleteDialog.open || deleteDialog.sprintId === null) return
  if (deleteDialogSubmitting.value) return
  deleteDialogSubmitting.value = true
  try {
    const response = await api.sprintDelete({
      sprint: deleteDialog.sprintId,
      cleanup_missing: true,
    })
    const name = deleteDialog.label.trim().length
      ? deleteDialog.label
      : `Sprint #${deleteDialog.sprintId}`
    showToast(`Deleted ${name}`)
    if (response.removed_references > 0) {
      const referencePlural = response.removed_references === 1 ? '' : 's'
      const taskPlural = response.updated_tasks === 1 ? 'task' : 'tasks'
      showToast(
        `Removed ${response.removed_references} sprint reference${referencePlural} across ${response.updated_tasks} ${taskPlural}.`,
      )
    }
    notifyIntegrity(response.integrity ?? null)
    closeDeleteDialog(true)
    await refreshAll(true)
  } catch (error: any) {
    showToast(error?.message || 'Failed to delete sprint')
  } finally {
    deleteDialogSubmitting.value = false
    if (!deleteDialog.open) {
      resetDeleteDialog()
    }
  }
}

function handleKeydown(event: KeyboardEvent) {
  if (event.key === 'Escape') {
    event.preventDefault()
    if (deleteDialog.open) {
      closeDeleteDialog()
      return
    }
    if (modal.open) {
      closeModal()
    }
  }
}

function resetForm() {
  form.label = ''
  form.goal = ''
  form.plan_length = ''
  form.ends_at = ''
  form.starts_at = ''
  form.capacity_points = ''
  form.capacity_hours = ''
  form.overdue_after = ''
  form.notes = ''
  form.skip_defaults = false
}

function openCreate() {
  modal.mode = 'create'
  modal.sprintId = null
  resetForm()
  modal.open = true
}

function fillFormFromSprint(sprint: SprintListItem) {
  form.label = sprint.label ?? sprint.display_name ?? ''
  form.goal = sprint.goal ?? ''
  form.plan_length = sprint.plan_length ?? ''
  form.ends_at = toDateTimeInputValue(sprint.planned_end)
  form.starts_at = toDateTimeInputValue(sprint.planned_start)
  form.capacity_points = sprint.capacity_points != null ? String(sprint.capacity_points) : ''
  form.capacity_hours = sprint.capacity_hours != null ? String(sprint.capacity_hours) : ''
  form.overdue_after = sprint.overdue_after ?? ''
  form.notes = sprint.notes ?? ''
  form.skip_defaults = false
}

function openEdit(sprint: SprintListItem) {
  fillFormFromSprint(sprint)
  modal.mode = 'edit'
  modal.sprintId = sprint.id
  modal.open = true
}

function closeModal() {
  modal.open = false
  modal.sprintId = null
  resetForm()
}

async function submitModal() {
  if (submitting.value) return
  const trimmedLabel = toTrimmedString(form.label)
  if (!trimmedLabel) {
    showToast('Label is required')
    return
  }
  submitting.value = true
  try {
    if (modal.mode === 'create') {
      const payload: SprintCreateRequest = { label: trimmedLabel }
      const goal = toTrimmedString(form.goal)
      if (goal) payload.goal = goal
      const planLength = toTrimmedString(form.plan_length)
      if (planLength) payload.plan_length = planLength
      const endsAt = fromDateTimeInputValue(form.ends_at)
      if (endsAt) payload.ends_at = endsAt
      const startsAt = fromDateTimeInputValue(form.starts_at)
      if (startsAt) payload.starts_at = startsAt
      const overdueAfter = toTrimmedString(form.overdue_after)
      if (overdueAfter) payload.overdue_after = overdueAfter
      const notesValue = toStringValue(form.notes)
      if (toTrimmedString(notesValue)) payload.notes = notesValue
      const capacityPoints = parseNumberInput(form.capacity_points)
      if (capacityPoints !== null) payload.capacity_points = capacityPoints
      const capacityHours = parseNumberInput(form.capacity_hours)
      if (capacityHours !== null) payload.capacity_hours = capacityHours
      if (form.skip_defaults) payload.skip_defaults = true

      const response = await api.sprintCreate(payload)
      showToast(`Created ${response.sprint.display_name}`)
      if (response.applied_defaults?.length) {
        showToast(`Applied defaults: ${response.applied_defaults.join(', ')}`)
      }
      if (response.warnings?.length) {
        response.warnings.forEach((warning) => showToast(`Warning: ${warning}`))
      }
    } else if (modal.sprintId !== null) {
      const goalValue = toTrimmedString(form.goal)
      const planLength = toTrimmedString(form.plan_length)
      const endsAt = fromDateTimeInputValue(form.ends_at)
      const startsAt = fromDateTimeInputValue(form.starts_at)
      const overdueAfter = toTrimmedString(form.overdue_after)
      const notesValue = toStringValue(form.notes)
      const updatePayload: SprintUpdateRequest = {
        sprint: modal.sprintId,
        label: trimmedLabel,
        goal: goalValue,
        plan_length: planLength,
        ends_at: endsAt ?? '',
        starts_at: startsAt ?? '',
        overdue_after: overdueAfter,
        notes: notesValue,
        capacity_points: parseNumberInput(form.capacity_points),
        capacity_hours: parseNumberInput(form.capacity_hours),
      }
      const response = await api.sprintUpdate(updatePayload)
      showToast(`Updated ${response.sprint.display_name}`)
      if (response.warnings?.length) {
        response.warnings.forEach((warning) => showToast(`Warning: ${warning}`))
      }
    }
    closeModal()
    await refreshAll(true)
  } catch (error: any) {
    showToast(error?.message || (modal.mode === 'create' ? 'Failed to create sprint' : 'Failed to update sprint'))
  } finally {
    submitting.value = false
  }
}

function openAnalytics(initial?: SprintListItem) {
  const candidate = initial?.id ?? selectedAnalyticsSprintId.value ?? analyticsDefaultSprintId.value
  selectedAnalyticsSprintId.value = candidate ?? null
  analyticsModal.tab = 'health'
  analyticsModal.open = true
  if (candidate) {
    void ensureSprintAnalytics(true)
  }
}

function closeAnalytics() {
  analyticsModal.open = false
}

function selectAnalyticsSprint(id: number | null) {
  selectedAnalyticsSprintId.value = id
}

function setAnalyticsTab(tab: AnalyticsTab) {
  analyticsModal.tab = tab
}

function setBurndownMetric(metric: SprintMetric) {
  analyticsModal.burndownMetric = metric
}

function setVelocityMetric(metric: SprintMetric) {
  if (analyticsModal.velocityMetric === metric) return
  analyticsModal.velocityMetric = metric
  if (analyticsModal.open) {
    void ensureVelocity(false)
  }
}

async function ensureSprintAnalytics(force = false) {
  const sprintId = selectedAnalyticsSprintId.value
  if (!sprintId) return
  try {
    await sprintAnalytics.fetchSprintAnalytics(sprintId, { force })
  } catch (error: any) {
    const message = error?.message || 'Failed to load sprint analytics'
    if (force) {
      showToast(message)
    } else {
      console.warn(message, error)
    }
  }
}

async function ensureVelocity(force = false) {
  try {
    await sprintAnalytics.loadVelocity(velocityParams.value, { force })
  } catch (error: any) {
    const message = error?.message || 'Failed to load sprint velocity'
    if (force) {
      showToast(message)
    } else {
      console.warn(message, error)
    }
  }
}

async function refreshAnalytics() {
  await Promise.all([ensureSprintAnalytics(true), ensureVelocity(true)])
}

function focusBacklogIfRequested() {
  const wantsBacklog = route.hash === '#backlog' || route.query.backlog === '1'
  if (!wantsBacklog) return
  nextTick(() => {
    const el = backlogRef.value?.$el as HTMLElement | undefined
    el?.scrollIntoView({ behavior: 'smooth', block: 'start' })
  })
}

onMounted(() => {
  if (typeof window !== 'undefined') {
    window.addEventListener('click', handleColumnMenuClick)
    window.addEventListener('keydown', handleColumnMenuKey)
  }
  bindCopyModifierListeners()
  void (async () => {
    await refreshAll(true)
    focusBacklogIfRequested()
  })()
})

onUnmounted(() => {
  removeModalListeners()
  if (typeof window !== 'undefined') {
    window.removeEventListener('click', handleColumnMenuClick)
    window.removeEventListener('keydown', handleColumnMenuKey)
    if (filterTimer.value !== null) {
      window.clearTimeout(filterTimer.value)
    }
  }
  unbindCopyModifierListeners()
})
</script>

<style scoped>
.header {
  justify-content: space-between;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}

.alert.warn {
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border-left: 4px solid var(--color-warning, #f59e0b);
  background: color-mix(in oklab, var(--color-warning, #f59e0b) 12%, transparent);
  border-radius: var(--radius-md, 0.375rem);
}

.filter-card {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 0;
  position: relative;
}

.filter-field {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 160px;
}

.icon-only {
  width: 36px;
  height: 36px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 20px;
  padding: 0;
}

.filter-meta {
  display: flex;
  flex-direction: column;
  gap: 8px;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, #6b7280);
}

.filter-meta__primary {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  align-items: flex-end;
}

.filter-meta__actions {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  margin-left: auto;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.filter-checkbox {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  align-self: flex-end;
  padding-bottom: 4px;
}

.filter-checkbox input {
  margin: 0;
}

.filter-columns-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 2.25rem;
}

.columns-popover {
  position: absolute;
  right: 16px;
  top: 100%;
  margin-top: 8px;
  padding: 12px;
  min-width: 220px;
  z-index: 20;
  box-shadow: var(--shadow-md, 0 12px 30px rgba(15, 23, 42, 0.2));
}

.hint {
  margin: -4px 0 0;
}

.group-stack {
  width: 100%;
}

.sprint-group {
  position: relative;
  transition: border-color 0.16s ease, background 0.16s ease;
}

.sprint-group--drop {
  border: 1px dashed var(--color-accent, #0ea5e9);
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 10%, transparent);
}

.group-header {
  display: flex;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}

.collapse-btn {
  border: none;
  background: transparent;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 24px;
  height: 24px;
  cursor: pointer;
  color: inherit;
}

.group-title {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.group-title__heading {
  display: flex;
  align-items: center;
  gap: 10px;
  flex-wrap: wrap;
}

.group-title h2 {
  margin: 0;
  font-size: var(--text-lg, 1.125rem);
}

.group-meta {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 18px;
  align-items: center;
}

.meta-item {
  display: inline-flex;
  align-items: baseline;
  gap: 6px;
  padding: 2px 0;
  font-size: var(--text-sm, 0.875rem);
  position: relative;
}

.meta-item--notes {
  align-items: center;
}

.meta-item--warning .meta-label {
  color: color-mix(in oklab, var(--color-warning, #f59e0b) 55%, transparent);
}

.meta-item--danger .meta-label {
  color: color-mix(in oklab, var(--color-danger, #ef4444) 65%, transparent);
}

.meta-label {
  font-size: var(--text-xs, 0.75rem);
  letter-spacing: 0.08em;
  text-transform: uppercase;
  color: color-mix(in oklab, var(--color-muted, #64748b) 90%, transparent);
}

.meta-value {
  font-weight: 600;
  color: var(--color-fg, var(--fg));
  line-height: 1.3;
}

.meta-item--hover {
  cursor: help;
  position: relative;
}

.meta-item--hover:focus-visible {
  outline: 2px solid color-mix(in oklab, var(--color-accent, #0ea5e9) 60%, transparent);
  outline-offset: 2px;
}

.meta-item--hover::after {
  content: attr(data-hover);
  position: absolute;
  left: 0;
  bottom: calc(100% + 12px);
  display: block;
  min-width: min(280px, 60vw);
  max-width: min(640px, 85vw);
  padding: 10px 12px;
  border-radius: 10px;
  background: var(--surface, #ffffff);
  border: 1px solid color-mix(in oklab, var(--color-border, #e2e8f0) 90%, transparent);
  box-shadow: var(--shadow-md, 0 16px 32px rgba(15, 23, 42, 0.18));
  color: var(--color-fg, var(--fg));
  font-size: var(--text-sm, 0.875rem);
  line-height: 1.45;
  white-space: pre-line;
  overflow-wrap: anywhere;
  word-break: break-word;
  opacity: 0;
  visibility: hidden;
  pointer-events: none;
  transform: translateY(6px);
  transition: opacity 120ms ease, visibility 120ms ease, transform 120ms ease;
  z-index: 32;
}

.meta-item--notes::after {
  min-width: min(360px, 70vw);
  max-width: min(720px, 85vw);
}

.meta-item--hover::before {
  content: '';
  position: absolute;
  left: 14px;
  bottom: calc(100% + 6px);
  width: 10px;
  height: 10px;
  background: var(--surface, #ffffff);
  border-left: 1px solid color-mix(in oklab, var(--color-border, #e2e8f0) 90%, transparent);
  border-top: 1px solid color-mix(in oklab, var(--color-border, #e2e8f0) 90%, transparent);
  transform: rotate(45deg) translateY(6px);
  opacity: 0;
  visibility: hidden;
  transition: opacity 120ms ease, visibility 120ms ease, transform 120ms ease;
  z-index: 31;
}

.meta-item--hover:hover::after,
.meta-item--hover:focus-visible::after,
.meta-item--hover:hover::before,
.meta-item--hover:focus-visible::before {
  opacity: 1;
  visibility: visible;
  transform: translateY(0);
}

.meta-item--truncate .meta-value,
.meta-value--truncate {
  display: inline-block;
  max-width: clamp(160px, 25vw, 240px);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  vertical-align: bottom;
}

.meta-value--icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  padding: 2px;
  color: var(--color-accent, #0ea5e9);
}

.meta-value--warning {
  color: var(--color-warning, #f59e0b);
}

.meta-value--danger {
  color: var(--color-danger, #ef4444);
}

.meta-icon {
  display: block;
}

.group-count {
  margin-left: auto;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, #6b7280);
}

.group-actions {
  display: flex;
  gap: 8px;
}

.group-body {
  margin-top: 12px;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.table-wrapper {
  width: 100%;
  overflow-x: auto;
}

.sprint-table {
  width: 100%;
  border-collapse: collapse;
  min-width: 640px;
}

.sprint-table th,
.sprint-table td {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-bottom: 1px solid var(--color-border, var(--border));
  vertical-align: middle;
}

.sprint-table th {
  text-align: left;
  font-weight: 600;
  color: var(--color-muted, var(--muted));
}

.sprint-table th.sortable {
  cursor: default;
}

.sprint-table th[data-column='id'],
.sprint-table td.task-cell--id {
  width: 110px;
}

.sprint-table th[data-column='title'],
.sprint-table td.task-cell--title {
  min-width: 260px;
  width: 40%;
}

.sprint-table th[data-column='status'],
.sprint-table td.task-cell--status {
  width: 140px;
}

.sprint-table th[data-column='priority'],
.sprint-table td.task-cell--priority {
  width: 130px;
}

.sprint-table th[data-column='assignee'],
.sprint-table td.task-cell--assignee {
  width: 160px;
}

.sprint-table th[data-column='due_date'],
.sprint-table td.task-cell--due_date {
  width: 150px;
}

.sprint-table th[data-column='modified'],
.sprint-table td.task-cell--modified {
  width: 170px;
}

.header-button {
  background: transparent;
  border: none;
  padding: 0;
  margin: 0;
  display: inline-flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  font: inherit;
  color: inherit;
  cursor: pointer;
}

.sort-glyph {
  font-size: 0.75rem;
  opacity: 0.8;
}

.sortable.active {
  color: var(--color-fg, var(--fg));
}

.task-row {
  cursor: grab;
  transition: background 0.16s ease;
}

.task-row:hover {
  background: color-mix(in oklab, var(--color-border, #e2e8f0) 40%, transparent);
}

.task-row--dragging {
  opacity: 0.6;
}

.task-cell {
  white-space: nowrap;
}

.task-cell--title {
  white-space: normal;
}

.task-cell--id {
  display: inline-flex;
  gap: 6px;
  align-items: baseline;
}

.task-title {
  font-weight: 600;
}

.task-title__badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  margin-left: 6px;
  cursor: default;
}

.status[data-status] {
  text-transform: capitalize;
}

.text-overdue {
  color: var(--color-danger, #ef4444);
  font-weight: 600;
}

.badge {
  display: inline-flex;
  align-items: center;
  padding: 0.15rem 0.45rem;
  border-radius: 999px;
  font-size: var(--text-xs, 0.75rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
}

.group-header > .badge {
  align-self: flex-start;
}

.badge--info {
  background: color-mix(in oklab, var(--color-accent, #0ea5e9) 18%, transparent);
  color: var(--color-accent, #0ea5e9);
}

.badge--danger {
  background: color-mix(in oklab, var(--color-danger, #ef4444) 18%, transparent);
  color: var(--color-danger, #ef4444);
}

.badge--success {
  background: color-mix(in oklab, var(--color-success, #16a34a) 18%, transparent);
  color: var(--color-success, #166534);
}

.badge--muted {
  background: color-mix(in oklab, var(--color-muted, #6b7280) 20%, transparent);
  color: var(--color-muted, #6b7280);
}

.filter-meta__actions .create-sprint-button {
  font-weight: 600;
  height: 2.25rem;
  padding: 0 var(--space-4, 1rem);
  gap: var(--space-2, 0.5rem);
}

.create-sprint-button:hover {
  background: var(--color-accent, #0ea5e9);
  color: var(--color-accent-contrast, #ffffff);
  border-color: transparent;
}

.create-sprint-button:hover .icon-glyph {
  color: inherit;
}

.sprint-delete-button {
  background: color-mix(in oklab, var(--color-surface, #f8fafc) 94%, transparent);
  border-color: color-mix(in oklab, var(--color-border, #e2e8f0) 85%, transparent);
  color: var(--color-muted, #64748b);
  transition: background 120ms ease, border-color 120ms ease, color 120ms ease;
}

.sprint-delete-button:hover {
  background: color-mix(in oklab, var(--color-danger, #ef4444) 18%, transparent);
  color: var(--color-danger, #ef4444);
  border-color: color-mix(in oklab, var(--color-danger, #ef4444) 45%, transparent);
}

.sprint-delete-button:hover .icon-glyph {
  color: inherit;
}

.empty-placeholder {
  margin: 0;
}

.btn.small {
  padding: 4px 10px;
  font-size: var(--text-sm, 0.875rem);
}

.sprint-modal__overlay {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5, 2rem);
  z-index: 50;
}

.sprint-modal__card {
  width: min(720px, 100%);
  max-height: 90vh;
  overflow-y: auto;
  padding: var(--space-5, 2rem);
  box-shadow: var(--shadow-lg, 0 20px 50px rgba(15, 23, 42, 0.35));
}

.sprint-modal__form {
  gap: var(--space-4, 1rem);
}

.sprint-modal__header {
  justify-content: space-between;
  align-items: flex-start;
  gap: var(--space-3, 0.75rem);
}

.collapse-enter-active,
.collapse-leave-active {
  transition: opacity 0.16s ease;
}

.collapse-enter-from,
.collapse-leave-to {
  opacity: 0;
}

@media (max-width: 720px) {
  .sprint-table {
    min-width: 560px;
  }
  .filter-meta {
    flex-direction: column;
    align-items: flex-start;
  }
  .group-actions {
    width: 100%;
    justify-content: flex-start;
  }
}
</style>
