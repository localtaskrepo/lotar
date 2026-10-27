<template>
  <Teleport to="body">
    <Transition name="task-panel">
      <div v-if="open" class="task-panel__overlay" @click.self="closePanel">
        <aside
          class="task-panel"
          role="dialog"
          aria-modal="true"
          :aria-label="mode === 'create' ? 'Create task' : `Task ${form.id || ''}`"
          :class="{ 'task-panel--drag-active': attachmentsDragActive }"
          @dragenter.capture="onAttachmentsDragEnter"
          @dragover.capture="onAttachmentsDragOver"
          @dragleave.capture="onAttachmentsDragLeave"
          @drop.capture="onAttachmentsDrop"
        >
          <header class="task-panel__header">
            <div class="task-panel__title">
              <span v-if="mode === 'edit'" class="task-panel__id">{{ form.id }}</span>
              <h2>{{ mode === 'create' ? 'Create task' : form.title || 'Task details' }}</h2>
              <span
                v-if="form.status"
                class="badge task-panel__status-badge"
                :class="statusBadgeClass"
                data-testid="task-panel-status-badge"
                >{{ form.status }}</span
              >
              <span v-if="isDeleted" class="badge task-panel__deleted-badge" data-testid="task-panel-deleted-badge"
                >Deleted</span
              >
            </div>
            <div class="task-panel__header-actions">
              <UiButton
                v-if="mode === 'edit' && hasSyncRemotes"
                class="task-panel__header-sync"
                variant="ghost"
                type="button"
                :disabled="syncDialogSubmitting"
                @click="openSyncDialog"
              >
                <IconGlyph name="refresh" />
                <span>Sync issue</span>
              </UiButton>
              <UiButton
                variant="ghost"
                icon-only
                type="button"
                aria-label="Close panel"
                title="Close panel"
                @click="closePanel"
              >
                <IconGlyph name="close" />
              </UiButton>
            </div>
          </header>

          <section class="task-panel__body" v-if="!loading">
            <div v-if="isDeleted" class="task-panel__deleted-banner" role="alert" data-testid="task-deleted-banner">
              <div class="task-panel__deleted-banner-text">
                <strong>Deleted{{ deletedAtLabel }}</strong>
                <span class="muted">
                  This task is in the trash and read-only. Restore it to edit, comment, or attach files.
                </span>
              </div>
              <div class="task-panel__deleted-banner-actions">
                <UiButton
                  variant="primary"
                  type="button"
                  data-testid="task-restore"
                  :disabled="restoreSubmitting"
                  @click="restoreDeletedTask"
                >
                  {{ restoreSubmitting ? 'Restoring…' : 'Restore' }}
                </UiButton>
                <UiButton
                  variant="danger"
                  type="button"
                  :disabled="restoreSubmitting"
                  @click="hardDeleteDialogOpen = true"
                >
                  Delete permanently…
                </UiButton>
              </div>
            </div>
            <form class="task-panel__form" @submit.prevent="handleSubmit">
              <!-- DEV-92: a deleted task is read-only; the fieldset disables
                   every nested form control in one place. Mutations are also
                   rejected server-side ("restore it first"). -->
              <fieldset class="task-panel__form-fieldset" :disabled="isDeleted">
                <TaskPanelSummarySection
                  :form="form"
                  :mode="mode"
                  :projects="projects"
                  :project-label="projectLabel"
                  :types="typeOptions"
                  :statuses="statusOptions"
                  :priorities="priorityOptions"
                  @fieldBlur="handleFieldBlur"
                  @projectChange="onProjectChange"
                  @updateStatus="updateStatus"
                />

                <fieldset class="task-panel__group">
                  <legend>Details</legend>
                  <div ref="descriptionEditorRoot" class="task-panel__description" @focusout="onDescriptionFocusOut">
                    <div
                      v-if="!editingDescription"
                      class="task-panel__description-display"
                      tabindex="0"
                      role="button"
                      aria-label="Edit description"
                      @pointerdown.prevent="startEditingDescription"
                      @click="startEditingDescription"
                      @keydown.enter.prevent="startEditingDescription"
                      @keydown.space.prevent="startEditingDescription"
                    >
                      <MarkdownContent
                        v-if="(form.description || '').trim()"
                        :source="form.description || ''"
                        :attachment-task-id="task.id"
                      />
                      <span v-else class="muted">Click to add a description…</span>
                    </div>

                    <div v-else class="task-panel__description-editor">
                      <div class="task-panel__description-body">
                        <textarea
                          v-if="!descriptionPreview"
                          ref="descriptionTextarea"
                          v-model="form.description"
                          rows="5"
                          placeholder="Description"
                          @input="onDescriptionInput"
                          @paste="onDescriptionPaste"
                        ></textarea>
                        <div v-else class="task-panel__description-preview" tabindex="0">
                          <MarkdownContent :source="form.description || ''" :attachment-task-id="task.id" />
                        </div>
                      </div>
                      <div class="task-panel__description-controls">
                        <UiButton
                          variant="ghost"
                          type="button"
                          class="task-panel__description-preview-toggle"
                          data-testid="description-preview-toggle"
                          @click="descriptionPreview = !descriptionPreview"
                        >
                          {{ descriptionPreview ? 'Hide preview' : 'Preview' }}
                        </UiButton>
                      </div>
                    </div>
                  </div>

                  <div
                    v-if="
                      showAttachmentsPreference &&
                      (canAttachFiles ||
                        attachments.length ||
                        attachmentsUploading ||
                        (showLinksInAttachmentsPreference && (linkReferences.length || addingLinkReferences)))
                    "
                    class="task-panel__attachments"
                  >
                    <div class="task-panel__attachments-header">
                      <span>Attachments</span>
                      <span v-if="attachmentsUploading" class="muted">Uploading…</span>
                      <template v-else-if="canAttachFiles">
                        <input
                          ref="attachmentPicker"
                          type="file"
                          multiple
                          hidden
                          data-testid="attachment-picker"
                          @change="onAttachmentPicked"
                        />
                        <UiButton
                          variant="ghost"
                          type="button"
                          class="task-panel__attach-button"
                          title="Attach files (or drop them here, or paste a screenshot)"
                          @click="attachmentPicker?.click()"
                        >
                          <IconGlyph name="paperclip" />
                          Attach
                        </UiButton>
                      </template>
                    </div>
                    <ul v-if="uploadProgress.length" class="task-panel__upload-progress" aria-label="Upload progress">
                      <li
                        v-for="item in uploadProgress"
                        :key="item.key"
                        class="task-panel__upload-row"
                        :class="`task-panel__upload-row--${item.state}`"
                      >
                        <span class="task-panel__upload-name">{{ item.name }}</span>
                        <progress
                          class="task-panel__upload-bar"
                          max="1"
                          :value="item.fraction"
                          :aria-label="`Uploading ${item.name}`"
                        ></progress>
                        <span class="task-panel__upload-state">{{ uploadStateLabel(item) }}</span>
                      </li>
                    </ul>
                    <button
                      v-if="canAttachFiles && !visibleAttachments.length && !attachmentsUploading"
                      type="button"
                      class="task-panel__attachments-empty"
                      @click="attachmentPicker?.click()"
                    >
                      Drop files here, paste a screenshot, or <span class="task-panel__attachments-browse">browse</span>
                    </button>

                    <ul v-if="galleryAttachments.length" class="task-panel__gallery">
                      <li
                        v-for="entry in galleryAttachments"
                        :key="entry.attachment || ''"
                        class="task-panel__gallery-item"
                        :class="{ 'task-panel__gallery-item--drop': dropTarget === entry.attachment }"
                        :draggable="canAttachFiles"
                        @dragstart="onReorderDragStart($event, 'gallery', entry.attachment || '')"
                        @dragover="onReorderDragOver($event, 'gallery', entry.attachment || '')"
                        @dragleave="onReorderDragLeave(entry.attachment || '')"
                        @drop="onReorderDrop($event, 'gallery', entry.attachment || '')"
                        @dragend="onReorderDragEnd"
                      >
                        <a
                          class="task-panel__attachment-link task-panel__gallery-link"
                          :href="attachmentUrl(entry.attachment || '')"
                          target="_blank"
                          rel="noopener"
                          :title="attachmentTooltip(entry.attachment || '')"
                          @click="openViewer($event, entry.attachment || '')"
                          @keydown="onReorderKey($event, 'gallery', entry.attachment || '')"
                        >
                          <span class="task-panel__gallery-preview">
                            <AttachmentTile
                              class="task-panel__gallery-image"
                              :attachment="attachmentView(entry.attachment || '')"
                              :alt="attachmentDisplayName(entry.attachment || '')"
                              :title="attachmentTooltip(entry.attachment || '')"
                              :preview-width="240"
                              cover
                            />
                          </span>
                          <span class="task-panel__attachment-name task-panel__gallery-name">{{
                            attachmentDisplayName(entry.attachment || '')
                          }}</span>
                        </a>
                        <span v-if="coverPath === entry.attachment" class="task-panel__cover-badge">Cover</span>
                        <span class="task-panel__gallery-actions">
                          <UiButton
                            v-if="
                              canAttachFiles && coverPath !== entry.attachment && isImagePath(entry.attachment || '')
                            "
                            variant="ghost"
                            icon-only
                            type="button"
                            class="task-panel__gallery-action"
                            aria-label="Use as cover"
                            title="Use as card cover"
                            @click.prevent.stop="useAsCover(entry.attachment || '')"
                          >
                            <IconGlyph name="star" />
                          </UiButton>
                          <UiButton
                            variant="ghost"
                            icon-only
                            type="button"
                            class="task-panel__attachment-remove task-panel__gallery-action"
                            aria-label="Remove attachment"
                            title="Remove attachment"
                            :disabled="attachmentsUploading || removingAttachmentPath === (entry.attachment || '')"
                            @click.prevent.stop="deferredRemoval.remove(entry.attachment || '')"
                          >
                            <IconGlyph name="close" />
                          </UiButton>
                        </span>
                      </li>
                    </ul>

                    <ul v-if="fileAttachments.length" class="task-panel__attachments-list">
                      <li
                        v-for="entry in fileAttachments"
                        :key="entry.attachment || ''"
                        class="task-panel__attachment"
                        :class="{ 'task-panel__attachment--drop': dropTarget === entry.attachment }"
                        :draggable="canAttachFiles"
                        @dragstart="onReorderDragStart($event, 'files', entry.attachment || '')"
                        @dragover="onReorderDragOver($event, 'files', entry.attachment || '')"
                        @dragleave="onReorderDragLeave(entry.attachment || '')"
                        @drop="onReorderDrop($event, 'files', entry.attachment || '')"
                        @dragend="onReorderDragEnd"
                      >
                        <div class="task-panel__attachment-chip">
                          <a
                            class="task-panel__attachment-link"
                            :href="attachmentUrl(entry.attachment || '')"
                            target="_blank"
                            rel="noopener"
                            :title="attachmentTooltip(entry.attachment || '')"
                            @click="openViewer($event, entry.attachment || '')"
                            @keydown="onReorderKey($event, 'files', entry.attachment || '')"
                          >
                            <AttachmentTile
                              :attachment="attachmentView(entry.attachment || '')"
                              :size="22"
                              :title="attachmentDisplayName(entry.attachment || '')"
                            />
                            <span class="task-panel__attachment-name">{{
                              attachmentDisplayName(entry.attachment || '')
                            }}</span>
                          </a>
                          <UiButton
                            variant="ghost"
                            icon-only
                            type="button"
                            class="task-panel__attachment-remove"
                            aria-label="Remove attachment"
                            title="Remove attachment"
                            :disabled="attachmentsUploading || removingAttachmentPath === (entry.attachment || '')"
                            @click.prevent.stop="deferredRemoval.remove(entry.attachment || '')"
                          >
                            <IconGlyph name="close" />
                          </UiButton>
                        </div>
                      </li>
                    </ul>
                    <AttachmentViewer
                      v-model:open-index="viewerIndex"
                      :attachments="viewerAttachments"
                      :info="attachmentInfo"
                    />

                    <div v-if="showLinksInAttachmentsPreference && (linkReferences.length || addingLinkReferences)">
                      <div class="task-panel__attachments-header">
                        <span>Links</span>
                        <span v-if="addingLinkReferences" class="muted">Adding…</span>
                      </div>

                      <ul class="task-panel__attachments-list">
                        <li v-for="entry in linkReferences" :key="entry.link || ''" class="task-panel__attachment">
                          <div class="task-panel__attachment-chip">
                            <a
                              class="task-panel__attachment-link"
                              :href="(entry.link || '').trim()"
                              target="_blank"
                              rel="noopener"
                              :title="(entry.link || '').trim()"
                            >
                              <span class="task-panel__attachment-icon" aria-hidden="true">
                                <IconGlyph name="chevron-right" />
                              </span>
                              <span class="task-panel__attachment-name">{{ linkDisplayName(entry.link || '') }}</span>
                            </a>
                            <UiButton
                              variant="ghost"
                              icon-only
                              type="button"
                              class="task-panel__attachment-remove"
                              aria-label="Remove link"
                              title="Remove link"
                              :disabled="
                                attachmentsUploading ||
                                addingLinkReferences ||
                                removingLinkUrl === (entry.link || '').trim()
                              "
                              @click.prevent.stop="removeLinkReference(entry.link || '')"
                            >
                              <IconGlyph name="close" />
                            </UiButton>
                          </div>
                        </li>
                      </ul>
                    </div>
                  </div>
                  <TaskPanelTagEditor
                    ref="tagEditorRef"
                    :tags="form.tags"
                    :configured-tags="configTagOptions"
                    :known-tags="knownTags"
                    :allow-custom-tags="allowCustomTags"
                    @update:tags="setTags"
                    @discoveredTags="mergeKnownTags"
                  />
                </fieldset>

                <TaskPanelOwnershipSection
                  :form="form"
                  :ordered-known-users="orderedKnownUsers"
                  :whoami="whoami"
                  :reporter-selection="reporterSelection"
                  :assignee-selection="assigneeSelection"
                  :reporter-mode="reporterMode"
                  :assignee-mode="assigneeMode"
                  :reporter-custom="reporterCustom"
                  :assignee-custom="assigneeCustom"
                  @update:reporterSelection="setReporterSelection"
                  @update:assigneeSelection="setAssigneeSelection"
                  @update:reporterCustom="setReporterCustom"
                  @update:assigneeCustom="setAssigneeCustom"
                  @commitReporterCustom="commitReporterCustom"
                  @commitAssigneeCustom="commitAssigneeCustom"
                  @resetReporterSelection="resetReporterSelection"
                  @resetAssigneeSelection="resetAssigneeSelection"
                  @fieldBlur="handleFieldBlur"
                />

                <fieldset v-if="sprintsLoading || hasSprints" class="task-panel__group">
                  <legend>Sprints</legend>
                  <div v-if="sprintsLoading" class="task-panel__sprints-loading">
                    <UiLoader size="sm">Loading sprint info…</UiLoader>
                  </div>
                  <div v-else class="task-panel__sprint-area">
                    <ChipListField
                      class="task-panel__sprint-field"
                      :model-value="sprintChipLabels"
                      empty-label="Not assigned to a sprint."
                      add-label="Assign sprint"
                      add-behavior="external"
                      :removable="false"
                      :chip-class="null"
                      @add-click="openSprintDialog('add')"
                    >
                      <template #chip="{ index }">
                        <span
                          v-if="assignedSprints[index]"
                          :class="[
                            'task-panel__sprint-chip',
                            `task-panel__sprint-chip--${assignedSprints[index].state}`,
                            { 'task-panel__sprint-chip--missing': assignedSprints[index].missing },
                          ]"
                        >
                          {{ assignedSprints[index].label }}
                          <button
                            type="button"
                            class="task-panel__sprint-chip-remove"
                            :aria-label="`Remove ${assignedSprints[index].label}`"
                            :title="`Remove ${assignedSprints[index].label}`"
                            :disabled="removingSprintId === assignedSprints[index].id"
                            @click.stop="removeSprintChip(assignedSprints[index].id)"
                          >
                            <IconGlyph name="close" />
                          </button>
                        </span>
                      </template>
                    </ChipListField>
                    <p v-if="assignedSprintNotice" class="task-panel__sprint-warning">
                      {{ assignedSprintNotice }}
                    </p>
                  </div>
                </fieldset>

                <TaskPanelCustomFieldsSection
                  :custom-fields="customFields"
                  :custom-field-keys="customFieldKeys"
                  :new-field-key="newField.key"
                  :new-field-value="newField.value"
                  :allow-new-fields="allowNewCustomFields"
                  @updateCustomFieldKey="updateCustomFieldKey"
                  @updateCustomFieldValue="updateCustomFieldValue"
                  @updateNewFieldKey="updateNewFieldKey"
                  @updateNewFieldValue="updateNewFieldValue"
                  @addField="addField"
                  @removeField="removeField"
                  @commit="commitCustomFields"
                />

                <section class="task-panel__group task-panel__activity">
                  <div class="task-panel__tabs" role="tablist">
                    <button
                      v-for="tab in activityTabs"
                      :key="tab.id"
                      type="button"
                      class="task-panel__tab"
                      :class="{ 'task-panel__tab--active': activityTab === tab.id }"
                      role="tab"
                      :aria-selected="activityTab === tab.id"
                      @click="activityTab = tab.id"
                    >
                      {{ tab.label }}
                    </button>
                  </div>

                  <TaskPanelCommentsTab
                    v-if="activityTab === 'comments'"
                    :mode="mode"
                    :task="task"
                    :new-comment="newComment"
                    :submitting="submitting"
                    :editing-comment-index="editingCommentIndex"
                    :editing-comment-text="editingCommentText"
                    :editing-comment-submitting="editingCommentSubmitting"
                    :format-date="formatDate"
                    :set-editing-textarea="setEditingCommentTextarea"
                    @reload="reloadTask"
                    @startEdit="startEditComment"
                    @saveEdit="saveCommentEdit"
                    @cancelEdit="cancelEditComment"
                    @addComment="addComment"
                    @update:newComment="updateNewComment"
                    @update:editingCommentText="updateEditingCommentText"
                  />

                  <TaskPanelRelationshipsTab
                    v-else-if="activityTab === 'relationships'"
                    :mode="mode"
                    :relationships="relationships"
                    :task-id="task.id"
                    :project="form.project"
                    :disabled="isDeleted || !ready"
                    :mutate="mutateRelationship"
                  />

                  <TaskPanelHistoryTab
                    v-else-if="activityTab === 'history'"
                    :mode="mode"
                    :change-log="changeLog"
                    :format-date="formatDate"
                    :format-field-name="formatFieldName"
                    :format-change-value="formatChangeValue"
                    @reload="reloadTask"
                  />

                  <TaskPanelCommitsTab
                    v-else-if="activityTab === 'commits'"
                    :mode="mode"
                    :commit-history="commitHistory"
                    :commits-loading="commitsLoading"
                    :format-commit="formatCommit"
                    :format-date="formatDate"
                    @refresh="refreshCommits"
                  />

                  <TaskPanelReferencesTab
                    v-else
                    :mode="mode"
                    :task="task"
                    :attachments-dir="attachmentsDir"
                    :hovered-reference-code="hoveredReferenceCode"
                    :hovered-reference-style="hoveredReferenceStyle"
                    :hovered-reference-loading="hoveredReferenceLoading"
                    :hovered-reference-error="hoveredReferenceError"
                    :hovered-reference-snippet="hoveredReferenceSnippet"
                    :hovered-reference-can-expand="hoveredReferenceCanExpand"
                    :hovered-reference-can-expand-before="hoveredReferenceCanExpandBefore"
                    :hovered-reference-can-expand-after="hoveredReferenceCanExpandAfter"
                    :on-reference-enter="onReferenceEnter"
                    :on-reference-leave="onReferenceLeave"
                    :on-reference-preview-enter="onReferencePreviewEnter"
                    :on-reference-preview-leave="onReferencePreviewLeave"
                    :expand-reference-snippet="expandReferenceSnippet"
                    :is-reference-line-highlighted="isReferenceLineHighlighted"
                    :set-reference-preview-element="setReferencePreviewElement"
                    @reload="reloadTask"
                    @updated="onReferencesUpdated"
                  />
                </section>

                <footer class="task-panel__footer">
                  <div v-if="Object.keys(errors).length" class="task-panel__errors">
                    <p v-for="(message, field) in errors" :key="field">{{ message }}</p>
                  </div>
                  <UiButton v-if="mode === 'create' && errors.config" type="button" @click="onProjectChange"
                    >Retry configuration</UiButton
                  >
                  <UiButton v-if="mode === 'create'" variant="primary" type="submit" :disabled="!canCreate">
                    {{ submitting ? 'Creating…' : 'Create task' }}
                  </UiButton>
                </footer>
              </fieldset>
            </form>
          </section>

          <section v-else class="task-panel__loading">
            <UiLoader size="md">Loading task…</UiLoader>
          </section>

          <div v-if="attachmentsDragActive" class="task-panel__drop-overlay" aria-hidden="true">
            <div class="task-panel__drop-overlay-card">
              <strong>{{ attachmentsDropLabel }}</strong>
              <span v-if="attachmentsUploading" class="muted">Uploading…</span>
              <span v-else-if="addingLinkReferences" class="muted">Adding…</span>
            </div>
          </div>
        </aside>
      </div>
    </Transition>
  </Teleport>
  <UiModal
    :open="sprintDialogOpen"
    :dismissible="!sprintDialogSubmitting"
    :aria-label="sprintDialogTitle"
    size="md"
    @close="closeSprintDialog()"
  >
    <form class="task-panel-dialog__form task-panel-dialog__form--sprint" @submit.prevent="submitSprintDialog">
      <header class="task-panel-dialog__header">
        <h2>{{ sprintDialogTitle }}</h2>
        <UiButton
          variant="ghost"
          icon-only
          type="button"
          :disabled="sprintDialogSubmitting"
          aria-label="Close dialog"
          title="Close dialog"
          @click="closeSprintDialog"
        >
          <IconGlyph name="close" />
        </UiButton>
      </header>
      <label class="task-panel-dialog__field">
        <span class="muted">Sprint</span>
        <select
          class="input"
          v-model="sprintDialogSelection"
          :disabled="sprintDialogMode === 'add' && !dialogSprintOptions.length"
        >
          <option v-for="option in dialogSprintOptions" :key="option.value" :value="option.value">
            {{ option.label }}
          </option>
        </select>
      </label>
      <p v-if="sprintDialogMode === 'add' && !dialogSprintOptions.length" class="muted">No sprints available yet.</p>
      <label v-if="sprintDialogMode === 'add'" class="task-panel-dialog__checkbox">
        <input type="checkbox" v-model="sprintDialogAllowClosed" /> Allow assigning to closed sprints
      </label>
      <footer class="task-panel-dialog__footer">
        <UiButton
          variant="primary"
          type="submit"
          :disabled="sprintDialogSubmitting || (sprintDialogMode === 'add' && !dialogSprintOptions.length)"
        >
          {{ sprintDialogSubmitting ? (sprintDialogMode === 'add' ? 'Assigning…' : 'Removing…') : sprintDialogTitle }}
        </UiButton>
        <UiButton variant="ghost" type="button" :disabled="sprintDialogSubmitting" @click="closeSprintDialog">
          Cancel
        </UiButton>
      </footer>
    </form>
  </UiModal>
  <Teleport to="body">
    <div
      v-if="syncDialogOpen"
      class="task-panel-dialog__overlay task-panel__sync-dialog"
      role="dialog"
      aria-modal="true"
      aria-label="Sync issue"
      @click.self="closeSyncDialog"
    >
      <UiCard class="task-panel-dialog__card">
        <form class="task-panel-dialog__form" @submit.prevent="submitSyncDialog">
          <header class="task-panel-dialog__header">
            <h2>Sync issue</h2>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="syncDialogSubmitting"
              aria-label="Close dialog"
              title="Close dialog"
              @click="closeSyncDialog"
            >
              <IconGlyph name="close" />
            </UiButton>
          </header>

          <label class="task-panel-dialog__field">
            <span class="muted">Remote</span>
            <select class="input" v-model="syncDialogRemote" :disabled="syncRemoteEntries.length <= 1">
              <option v-for="entry in syncRemoteEntries" :key="entry.name" :value="entry.name">
                {{ entry.name }} · {{ formatRemote(entry.remote) }}
              </option>
            </select>
          </label>

          <label class="task-panel-dialog__field">
            <span class="muted">Direction</span>
            <select class="input" v-model="syncDialogAction">
              <option value="pull">Pull (update this task from remote)</option>
              <option value="push">Push (update remote from this task)</option>
            </select>
          </label>

          <label class="task-panel-dialog__checkbox">
            <input type="checkbox" v-model="syncDialogWriteReport" /> Write report to disk
          </label>

          <footer class="task-panel-dialog__footer">
            <UiButton variant="primary" type="submit" :disabled="syncDialogSubmitting || !syncDialogRemote">
              {{ syncDialogSubmitting ? 'Syncing…' : syncDialogAction === 'pull' ? 'Run pull' : 'Run push' }}
            </UiButton>
            <UiButton variant="ghost" type="button" :disabled="syncDialogSubmitting" @click="closeSyncDialog">
              Cancel
            </UiButton>
          </footer>
        </form>
      </UiCard>
    </div>
  </Teleport>
  <Teleport to="body">
    <div
      v-if="hardDeleteDialogOpen"
      class="task-panel-dialog__overlay"
      role="dialog"
      aria-modal="true"
      aria-label="Delete permanently"
      @click.self="hardDeleteDialogOpen = false"
    >
      <UiCard class="task-panel-dialog__card">
        <form class="task-panel-dialog__form" @submit.prevent="submitHardDelete">
          <header class="task-panel-dialog__header">
            <h2>Delete permanently</h2>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="hardDeleteSubmitting"
              aria-label="Close dialog"
              title="Close dialog"
              @click="hardDeleteDialogOpen = false"
            >
              <IconGlyph name="close" />
            </UiButton>
          </header>
          <p>
            Permanently delete {{ task.id || 'this task' }}? The task file and its history are removed. This cannot be
            undone.
          </p>
          <p class="muted">
            Attached repository files are kept; managed attachment blobs that are still referenced by other tasks are
            retained and reported after deletion.
          </p>
          <label class="task-panel-dialog__checkbox">
            <input type="checkbox" v-model="hardDeleteConfirmed" /> I understand this cannot be undone
          </label>
          <footer class="task-panel-dialog__footer">
            <UiButton variant="danger" type="submit" :disabled="hardDeleteSubmitting || !hardDeleteConfirmed">
              {{ hardDeleteSubmitting ? 'Deleting…' : 'Delete permanently' }}
            </UiButton>
            <UiButton
              variant="ghost"
              type="button"
              :disabled="hardDeleteSubmitting"
              @click="hardDeleteDialogOpen = false"
            >
              Cancel
            </UiButton>
          </footer>
        </form>
      </UiCard>
    </div>
  </Teleport>
</template>

<script setup lang="ts">
import { computed, nextTick, onUnmounted, ref, watch } from 'vue'
import { api } from '../api/client'
import type {
  AttachmentInfo,
  ConfigInspectResult,
  SprintIntegrityDiagnostics,
  SyncRemoteConfig,
  SyncRequest,
  TaskDTO,
} from '../api/types'
import { useTaskPanelState } from '../composables/useTaskPanelState'
import {
  pastedFiles,
  useTaskPanelAttachmentDrop,
  useTaskPanelAttachmentPreferences,
} from '../composables/task-panel/useTaskPanelAttachmentDrop'
import { useDeferredAttachmentRemoval } from '../composables/task-panel/useDeferredAttachmentRemoval'
import { useTaskPanelReferenceMutations } from '../composables/task-panel/useTaskPanelReferenceMutations'
import {
  attachmentDisplayName,
  attachmentUrl as buildAttachmentUrl,
  attachmentKind,
  extractUrlsFromText,
  formatFileSize,
  linkDisplayName,
  moveTo,
  reorderSubset,
  taskAttachments,
} from '../utils/taskAttachments'
import { applyTaskSnapshot, isDeletedTask, projectPrefixOfTaskId } from '../utils/text'
import { useTaskStore } from '../composables/useTaskStore'
import { setTaskPanelCommands } from '../composables/useTaskPanelController'
import { recordRecentTask } from '../utils/recentTasks'
import ChipListField from './ChipListField.vue'
import AttachmentTile from './AttachmentTile.vue'
import AttachmentViewer from './AttachmentViewer.vue'
import IconGlyph from './IconGlyph.vue'
import MarkdownContent from './MarkdownContent.vue'
import UiButton from './UiButton.vue'
import UiCard from './UiCard.vue'
import UiLoader from './UiLoader.vue'
import UiModal from './UiModal.vue'
import TaskPanelCommentsTab from './task-panel/TaskPanelCommentsTab.vue'
import TaskPanelCommitsTab from './task-panel/TaskPanelCommitsTab.vue'
import TaskPanelCustomFieldsSection from './task-panel/TaskPanelCustomFieldsSection.vue'
import TaskPanelHistoryTab from './task-panel/TaskPanelHistoryTab.vue'
import TaskPanelOwnershipSection from './task-panel/TaskPanelOwnershipSection.vue'
import TaskPanelReferencesTab from './task-panel/TaskPanelReferencesTab.vue'
import TaskPanelRelationshipsTab from './task-panel/TaskPanelRelationshipsTab.vue'
import TaskPanelSummarySection from './task-panel/TaskPanelSummarySection.vue'
import TaskPanelTagEditor from './task-panel/TaskPanelTagEditor.vue'
import { showToast } from './toast'

const props = defineProps<{
  open: boolean
  taskId?: string | null
  initialProject?: string | null
  initialStatus?: string | null
  initialDueDate?: string | null
  /** DEV-104: concrete sprint id prefilled into a create-mode draft. */
  initialSprint?: number | null
  focusSection?: string | null
  /** DEV-92: bumped by TaskPanelHost on external lifecycle transitions. */
  lifecycleReload?: number
}>()
const emit = defineEmits<{
  (e: 'close'): void
  (e: 'created', task: TaskDTO): void
  (e: 'updated', task: TaskDTO): void
  (e: 'restored', task: TaskDTO): void
}>()
const taskStore = useTaskStore()

const {
  mode,
  applyPatch,
  ready,
  projects,
  statusOptions,
  priorityOptions,
  typeOptions,
  loading,
  submitting,
  canCreate,
  task,
  form,
  statusBadgeClass,
  hoveredReferenceCode,
  hoveredReferenceStyle,
  hoveredReferenceLoading,
  hoveredReferenceError,
  hoveredReferenceSnippet,
  hoveredReferenceCanExpand,
  hoveredReferenceCanExpandBefore,
  hoveredReferenceCanExpandAfter,
  onReferenceEnter,
  onReferenceLeave,
  onReferencePreviewEnter,
  onReferencePreviewLeave,
  expandReferenceSnippet,
  isReferenceLineHighlighted,
  setReferencePreviewElement,
  errors,
  knownTags,
  allowCustomTags,
  configTagOptions,
  customFields,
  customFieldKeys,
  newField,
  allowNewCustomFields,
  activityTabs,
  activityTab,
  changeLog,
  commitHistory,
  commitsLoading,
  sprintsLoading,
  assignedSprints,
  hasAssignedSprints,
  assignedSprintNotice,
  sprintOptions,
  sprintLookup,
  hasSprints,
  relationships,
  mutateRelationship,
  mergeKnownTags,
  projectLabel,
  commitCustomFields,
  addField,
  removeField,
  setTags,
  updateCustomFieldKey,
  updateCustomFieldValue,
  updateNewFieldKey,
  updateNewFieldValue,
  handleFieldBlur: handleFieldBlurBase,
  onFieldBlur: onFieldBlurBase,
  closePanel,
  handleSubmit,
  updateStatus,
  reloadTask,
  invalidateTaskRequests,
  formatDate,
  formatCommit,
  formatFieldName,
  formatChangeValue,
  onProjectChange,
  refreshCommits,
  newComment,
  editingCommentIndex,
  editingCommentText,
  editingCommentSubmitting,
  setEditingCommentTextarea,
  updateNewComment,
  updateEditingCommentText,
  addComment,
  startEditComment,
  cancelEditComment,
  saveCommentEdit,
  reporterMode,
  assigneeMode,
  reporterCustom,
  assigneeCustom,
  orderedKnownUsers,
  reporterSelection,
  assigneeSelection,
  setReporterSelection,
  setAssigneeSelection,
  setReporterCustom,
  setAssigneeCustom,
  commitReporterCustom,
  commitAssigneeCustom,
  resetReporterSelection,
  resetAssigneeSelection,
  whoami,
  refreshSprints,
  attachmentsDir,
} = useTaskPanelState(props, emit)

// Named apart from the taskId prop, which it would otherwise shadow.
const panelTaskId = computed(() => (task.id || '').trim())

// DEV-198: offer this task's actions to the command palette and remember it
// as a recent task, while an existing, editable task is shown.
const paletteTaskId = computed(() =>
  mode.value === 'edit' && ready.value && panelTaskId.value && !isDeletedTask(task) ? panelTaskId.value : '',
)
watch(
  () => [paletteTaskId.value, task.title, task.status, task.priority, statusOptions.value, priorityOptions.value],
  () => {
    const id = paletteTaskId.value
    if (!id) {
      setTaskPanelCommands(null)
      return
    }
    recordRecentTask(id, task.title || '')
    setTaskPanelCommands({
      taskId: id,
      title: task.title || '',
      status: task.status || '',
      priority: task.priority || '',
      statuses: [...statusOptions.value],
      priorities: [...priorityOptions.value],
      setStatus: (status) => updateStatus(status),
      setPriority: (priority) => applyPatch({ priority }),
      assignToMe: () => applyPatch({ assignee: '@me' }),
    })
  },
  { immediate: true },
)
onUnmounted(() => setTaskPanelCommands(null))

// -- DEV-92: deleted (trash) task mode --------------------------------------

const isDeleted = computed(() => isDeletedTask(task))

const deletedAtLabel = computed(() => {
  const stamp = task.deleted_at
  if (!stamp) return ''
  try {
    return ` ${formatDate(stamp)}`
  } catch {
    return ''
  }
})

const restoreSubmitting = ref(false)

async function restoreDeletedTask() {
  if (!task.id || restoreSubmitting.value) return
  const id = task.id
  restoreSubmitting.value = true
  try {
    await taskStore.restore(id, projectPrefixOfTaskId(id) || undefined)
    if (!props.open || task.id !== id || props.taskId !== id) return
    const restored = taskStore._map.value.get(id)
    if (!restored || isDeletedTask(restored)) {
      await reloadTask()
      return
    }
    applyTaskSnapshot(task, restored)
    // 'restored' is the authoritative lifecycle-clearing channel: ordinary
    // 'updated' snapshots can no longer clear a live tombstone (DEV-92).
    emit('restored', restored)
    emit('updated', restored)
    showToast('Task restored')
    await reloadTask()
  } catch (error: any) {
    showToast(error?.message || 'Failed to restore task')
  } finally {
    restoreSubmitting.value = false
  }
}

const hardDeleteDialogOpen = ref(false)
const hardDeleteSubmitting = ref(false)
const hardDeleteConfirmed = ref(false)

watch(hardDeleteDialogOpen, (open) => {
  if (!open) hardDeleteConfirmed.value = false
})

async function submitHardDelete() {
  if (!task.id || hardDeleteSubmitting.value || !hardDeleteConfirmed.value) return
  hardDeleteSubmitting.value = true
  try {
    const response = await api.deleteTask(task.id, {
      project: projectPrefixOfTaskId(task.id) || undefined,
      hard: true,
    })
    const warnings = Array.isArray(response?.warnings) ? response.warnings : []
    warnings.forEach((warning) => showToast(warning))
    showToast('Task permanently deleted')
    hardDeleteDialogOpen.value = false
    closePanel()
  } catch (error: any) {
    showToast(error?.message || 'Failed to delete task')
  } finally {
    hardDeleteSubmitting.value = false
  }
}

// DEV-92: external delete/restore while this task's panel is open. The host
// bumps `lifecycleReload` from the SHARED store's reactivity (no duplicate
// SSE); only the currently loaded task reloads, discarding unsaved edits on
// external deletion (they are void — the server rejects writes on trash rows).
watch(
  () => props.lifecycleReload,
  async () => {
    if (!props.open) return
    if (mode.value !== 'edit' || !task.id) return
    if (props.taskId && task.id !== props.taskId) return
    invalidateTaskRequests()
    try {
      await reloadTask()
    } catch {
      // reloadTask surfaces its own failure state; the next lifecycle
      // transition or manual reopen retries.
    }
  },
)

/** DEV-92: trash rows reject attachment/link mutations before they start. */
function rejectDeletedMutation(label: string): boolean {
  if (!isDeleted.value) return false
  showToast(`Restore the task before ${label}`)
  return true
}

const sprintDialogOpen = ref(false)
const sprintDialogSubmitting = ref(false)
const sprintDialogMode = ref<'add' | 'remove'>('add')
const sprintDialogSelection = ref('active')
const sprintDialogAllowClosed = ref(false)
const removingSprintId = ref<number | null>(null)
const sprintChipLabels = computed(() => assignedSprints.value.map((entry) => entry.label))

// DEV-104: create-mode drafts can only carry concrete sprint ids — the
// active/next/previous server selectors resolve against a persisted task,
// which does not exist before the atomic create request.
const dialogSprintOptions = computed(() => {
  if (mode.value !== 'create') return sprintOptions.value
  return sprintOptions.value.filter((option) => {
    const id = Number(option.value)
    return Number.isInteger(id) && id > 0
  })
})

const syncConfigInspect = ref<ConfigInspectResult | null>(null)
const syncConfigProject = ref<string | null>(null)
const syncRemotes = computed<Record<string, SyncRemoteConfig>>(() => syncConfigInspect.value?.effective?.remotes ?? {})
const syncRemoteEntries = computed(() =>
  Object.entries(syncRemotes.value)
    .map(([name, remote]) => ({ name, remote }))
    .sort((a, b) => a.name.localeCompare(b.name)),
)
const hasSyncRemotes = computed(() => syncRemoteEntries.value.length > 0)

const syncDialogOpen = ref(false)
const syncDialogSubmitting = ref(false)
const syncDialogAction = ref<'pull' | 'push'>('pull')
const syncDialogRemote = ref('')
const syncDialogWriteReport = ref(true)

const editingDescription = ref(false)
const descriptionPreview = ref(false)
const descriptionEditorRoot = ref<HTMLElement | null>(null)
const descriptionTextarea = ref<HTMLTextAreaElement | null>(null)
const descriptionFocusoutTick = ref(0)

async function loadSyncConfig(prefix: string | null) {
  if (!prefix) {
    syncConfigInspect.value = null
    syncConfigProject.value = null
    return
  }
  if (syncConfigProject.value === prefix) return
  syncConfigProject.value = prefix
  try {
    syncConfigInspect.value = await api.inspectConfig(prefix)
  } catch {
    syncConfigInspect.value = null
  }
}

watch(
  panelTaskId,
  (value) => {
    const prefix = projectPrefixOfTaskId(value)
    loadSyncConfig(prefix)
  },
  { immediate: true },
)

const attachmentPreferences = useTaskPanelAttachmentPreferences()
const {
  showAttachments: showAttachmentsPreference,
  showLinksInAttachments: showLinksInAttachmentsPreference,
  autoDetectLinks: autoDetectLinksPreference,
} = attachmentPreferences

const referenceMutations = useTaskPanelReferenceMutations({
  task,
  mode,
  rejectDeletedMutation,
  emitUpdated: (updated) => emit('updated', updated),
})
const {
  attachments,
  linkReferences,
  attachmentsUploading,
  addingLinkReferences,
  removingAttachmentPath,
  removingLinkUrl,
  uploadProgress,
  applyReferencesFromTaskResponse,
  removeLinkReference,
} = referenceMutations

const {
  attachmentsDragActive,
  attachmentsDropLabel,
  onAttachmentsDragEnter,
  onAttachmentsDragOver,
  onAttachmentsDragLeave,
  onAttachmentsDrop,
  attachPickedFiles,
} = useTaskPanelAttachmentDrop({
  task,
  mode,
  taskId: panelTaskId,
  isDeleted,
  isOpen: () => props.open,
  targetTaskId: () => props.taskId,
  preferences: attachmentPreferences,
  mutations: referenceMutations,
})

function onReferencesUpdated(updated: TaskDTO) {
  applyTaskSnapshot(task, updated)
  applyReferencesFromTaskResponse(updated)
  emit('updated', updated)
}

// Files can be attached once the task exists and is not deleted.
const canAttachFiles = computed(() => mode.value === 'edit' && !isDeleted.value && Boolean(task.id))
const attachmentPicker = ref<HTMLInputElement | null>(null)
function onAttachmentPicked(event: Event) {
  const input = event.target as HTMLInputElement
  const files = Array.from(input.files ?? [])
  input.value = ''
  void attachPickedFiles(files)
}

const attachmentUrl = (relPath: string) => buildAttachmentUrl(relPath, task.id)
// Images get a preview gallery; other files keep compact chips with a type tile.
const attachmentView = (relPath: string) => taskAttachments({ id: task.id, references: [{ attachment: relPath }] })[0]!
const isImagePath = (relPath: string) => attachmentKind(relPath) === 'image'

// Removing from the Attachments area hides the file at once and offers Undo;
// the removal is sent when the toast expires, or right away when the panel
// closes or switches task (DEV-216).
const deferredRemoval = useDeferredAttachmentRemoval({
  currentTaskId: () => task.id,
  onRemoved: (updated) => onReferencesUpdated(updated),
})
watch(
  () => [props.open, props.taskId],
  () => deferredRemoval.flush(),
)

const visibleAttachments = computed(() =>
  attachments.value.filter((entry) => !deferredRemoval.isPending(entry.attachment || '')),
)
// Pictures and videos get preview tiles; other files keep compact chips.
const isVisual = (relPath: string) => ['image', 'video'].includes(attachmentKind(relPath))
const galleryAttachments = computed(() => visibleAttachments.value.filter((entry) => isVisual(entry.attachment || '')))
const fileAttachments = computed(() => visibleAttachments.value.filter((entry) => !isVisual(entry.attachment || '')))
/** The first image is the card cover on boards and hover cards. */
const coverPath = computed(
  () => visibleAttachments.value.find((entry) => isImagePath(entry.attachment || ''))?.attachment ?? null,
)

// In-app viewer over everything shown, gallery first.
const viewerIndex = ref<number | null>(null)
const viewerPaths = computed(() =>
  [...galleryAttachments.value, ...fileAttachments.value].map((entry) => entry.attachment || ''),
)
const viewerAttachments = computed(() => viewerPaths.value.map((path) => attachmentView(path)))
function openViewer(event: MouseEvent, relPath: string) {
  // Modified clicks keep the browser's own new-tab/download behaviour.
  if (event.metaKey || event.ctrlKey || event.shiftKey || event.altKey || event.button !== 0) return
  event.preventDefault()
  viewerIndex.value = Math.max(0, viewerPaths.value.indexOf(relPath))
}
watch(
  () => [props.open, props.taskId],
  () => {
    viewerIndex.value = null
  },
)

// Sizes and dimensions for tooltips and the viewer, fetched once per file.
const attachmentInfo = ref<Record<string, AttachmentInfo>>({})
watch(
  () => attachments.value.map((entry) => entry.attachment || '').join('|'),
  async () => {
    const missing = attachments.value
      .map((entry) => entry.attachment || '')
      .filter((path) => path && !attachmentInfo.value[path])
    if (!missing.length || !task.id) return
    try {
      const response = await api.attachmentInfo({
        project: projectPrefixOfTaskId(task.id) || undefined,
        attachments: missing,
      })
      const next = { ...attachmentInfo.value }
      for (const item of response.items) next[item.attachment] = item
      attachmentInfo.value = next
    } catch {
      // Tooltips just show the name.
    }
  },
  { immediate: true },
)
function attachmentTooltip(relPath: string): string {
  const parts = [attachmentDisplayName(relPath)]
  const info = attachmentInfo.value[relPath]
  if (info?.width && info.height) parts.push(`${info.width}×${info.height}`)
  if (info) parts.push(formatFileSize(info.size))
  return parts.join(' · ')
}

function uploadStateLabel(item: { state: string; fraction: number }): string {
  if (item.state === 'done') return 'Done'
  if (item.state === 'failed') return 'Failed'
  if (item.state === 'waiting') return 'Waiting'
  return `${Math.round(item.fraction * 100)}%`
}

// Reordering: drag tiles or chips, or Alt+Arrow on a focused one. Images and
// files reorder within their own group; the first image is the cover.
async function saveAttachmentOrder(order: string[]) {
  const current = attachments.value.map((entry) => entry.attachment || '')
  if (order.join('|') === current.join('|') || !task.id) return
  const before = (task.references ?? []).map((reference) => ({ ...reference }))
  // Show the new order right away; the server confirms it.
  const queue = [...order]
  task.references = (task.references ?? []).map((reference) =>
    reference.attachment ? { ...reference, attachment: queue.shift() } : reference,
  )
  try {
    onReferencesUpdated(await api.reorderTaskAttachments({ id: task.id, attachments: order }))
  } catch (error) {
    console.warn('Failed to reorder attachments', error)
    task.references = before
    showToast('Could not save the new attachment order')
  }
}
function groupPaths(group: 'gallery' | 'files'): string[] {
  return (group === 'gallery' ? galleryAttachments.value : fileAttachments.value).map((entry) => entry.attachment || '')
}
function moveWithinGroup(group: 'gallery' | 'files', path: string, index: number) {
  const all = attachments.value.map((entry) => entry.attachment || '')
  void saveAttachmentOrder(reorderSubset(all, moveTo(groupPaths(group), path, index)))
}
function useAsCover(path: string) {
  moveWithinGroup('gallery', path, 0)
}

const REORDER_MIME = 'application/x-lotar-attachment'
const dragging = ref<{ group: 'gallery' | 'files'; path: string } | null>(null)
const dropTarget = ref<string | null>(null)
function onReorderDragStart(event: DragEvent, group: 'gallery' | 'files', path: string) {
  if (!canAttachFiles.value || !event.dataTransfer) return
  // Drop the link/image data browsers add, so the panel's own drop zone does
  // not mistake the drag for a file or link to attach.
  event.dataTransfer.clearData()
  event.dataTransfer.setData(REORDER_MIME, path)
  event.dataTransfer.effectAllowed = 'move'
  dragging.value = { group, path }
}
function onReorderDragOver(event: DragEvent, group: 'gallery' | 'files', path: string) {
  if (dragging.value?.group !== group) return
  event.preventDefault()
  if (event.dataTransfer) event.dataTransfer.dropEffect = 'move'
  dropTarget.value = path === dragging.value.path ? null : path
}
function onReorderDragLeave(path: string) {
  if (dropTarget.value === path) dropTarget.value = null
}
function onReorderDrop(event: DragEvent, group: 'gallery' | 'files', path: string) {
  const source = dragging.value
  if (source?.group !== group) return
  event.preventDefault()
  event.stopPropagation()
  dropTarget.value = null
  dragging.value = null
  if (source.path !== path) moveWithinGroup(group, source.path, groupPaths(group).indexOf(path))
}
function onReorderDragEnd() {
  dragging.value = null
  dropTarget.value = null
}
function onReorderKey(event: KeyboardEvent, group: 'gallery' | 'files', path: string) {
  if (!event.altKey || !canAttachFiles.value) return
  if (event.key !== 'ArrowLeft' && event.key !== 'ArrowRight') return
  event.preventDefault()
  const paths = groupPaths(group)
  moveWithinGroup(group, path, paths.indexOf(path) + (event.key === 'ArrowLeft' ? -1 : 1))
}

// Pasting a screenshot into the description attaches it and links it inline.
async function onDescriptionPaste(event: ClipboardEvent) {
  const files = pastedFiles(event)
  if (!files.length) return
  event.preventDefault()
  // The panel-wide paste handler would attach the same files again.
  event.stopPropagation()
  if (!canAttachFiles.value) {
    showToast('Save the task before attaching files')
    return
  }
  const textarea = event.target as HTMLTextAreaElement
  const start = textarea.selectionStart ?? (form.description || '').length
  const end = textarea.selectionEnd ?? start
  const stored = await referenceMutations.uploadFiles(files)
  if (!stored.length) return
  const snippet = stored
    .map((path) => `${isImagePath(path) ? '!' : ''}[${attachmentDisplayName(path)}](attachment:${path})`)
    .join('\n')
  const text = form.description || ''
  form.description = text.slice(0, start) + snippet + text.slice(end)
  onDescriptionInput()
  // The editor may have closed while the upload ran; save in that case.
  if (!editingDescription.value) onFieldBlur('description')
}

async function detectAndAttachLinksFromTaskText() {
  if (!autoDetectLinksPreference.value) return
  if (!showLinksInAttachmentsPreference.value) return
  if (mode.value !== 'edit' || !task.id) return
  if (attachmentsUploading.value || addingLinkReferences.value) return

  const urls = extractUrlsFromText(`${form.title || ''}\n${form.description || ''}`)
  if (!urls.length) return
  await referenceMutations.attachLinksFromText(urls)
}

const handleFieldBlur = async (field: string) => {
  await handleFieldBlurBase(field)
  if (field === 'title') {
    await detectAndAttachLinksFromTaskText()
  }
}

const onFieldBlur = async (field: string) => {
  await onFieldBlurBase(field)
  if (field === 'description') {
    await detectAndAttachLinksFromTaskText()
  }
}

function autosizeDescriptionTextarea() {
  const el = descriptionTextarea.value
  if (!el) return

  el.style.height = 'auto'

  const computed = window.getComputedStyle(el)
  const minHeight = Number.parseFloat(computed.minHeight || '0') || 0
  const borderTop = Number.parseFloat(computed.borderTopWidth || '0') || 0
  const borderBottom = Number.parseFloat(computed.borderBottomWidth || '0') || 0

  let target = el.scrollHeight
  if (computed.boxSizing === 'border-box') {
    target += borderTop + borderBottom
  }

  if (!Number.isFinite(target) || target <= 0) {
    return
  }

  // If we're within a couple pixels of the min-height, prefer the min-height.
  // This avoids tiny scrollHeight rounding differences making edit mode look taller.
  const epsilon = 3
  if (minHeight > 0 && target <= minHeight + epsilon) {
    el.style.height = `${minHeight}px`
    return
  }

  el.style.height = `${target}px`
}

function onDescriptionInput() {
  nextTick(() => autosizeDescriptionTextarea())
}

function startEditingDescription() {
  if (editingDescription.value) return
  if (isDeleted.value) {
    showToast('Restore the task before editing the description')
    return
  }
  editingDescription.value = true
  descriptionPreview.value = false
  nextTick(() => {
    descriptionTextarea.value?.focus()
    autosizeDescriptionTextarea()
  })
}

// Pressed pointers, so leaving the editor mid-click can wait for the click.
const pressedPointers = new Set<number>()
const trackPointerDown = (event: PointerEvent) => pressedPointers.add(event.pointerId)
const trackPointerUp = (event: PointerEvent) => pressedPointers.delete(event.pointerId)
window.addEventListener('pointerdown', trackPointerDown, true)
window.addEventListener('pointerup', trackPointerUp, true)
window.addEventListener('pointercancel', trackPointerUp, true)
let pendingDescriptionClose: (() => void) | null = null
onUnmounted(() => {
  window.removeEventListener('pointerdown', trackPointerDown, true)
  window.removeEventListener('pointerup', trackPointerUp, true)
  window.removeEventListener('pointercancel', trackPointerUp, true)
  pendingDescriptionClose?.()
})

function onDescriptionFocusOut(_event: FocusEvent) {
  if (!editingDescription.value) return

  descriptionFocusoutTick.value += 1
  const tick = descriptionFocusoutTick.value

  const close = () => {
    if (tick !== descriptionFocusoutTick.value) return

    const active = document.activeElement as HTMLElement | null
    if (active && descriptionEditorRoot.value?.contains(active)) return

    editingDescription.value = false
    descriptionPreview.value = false
    onFieldBlur('description')
  }

  setTimeout(() => {
    if (!pressedPointers.size) {
      close()
      return
    }
    // A press moved focus away (e.g. onto "Create task" below). Closing now
    // shrinks the editor and moves that button before the click lands
    // (DEV-188), so wait until the press ends and its click has run.
    pendingDescriptionClose?.()
    const finish = () => {
      window.removeEventListener('pointerup', finish, true)
      window.removeEventListener('pointercancel', finish, true)
      if (pendingDescriptionClose === cancel) pendingDescriptionClose = null
      setTimeout(close, 0)
    }
    const cancel = () => {
      window.removeEventListener('pointerup', finish, true)
      window.removeEventListener('pointercancel', finish, true)
    }
    pendingDescriptionClose = cancel
    window.addEventListener('pointerup', finish, true)
    window.addEventListener('pointercancel', finish, true)
  }, 0)
}

watch(descriptionPreview, (isPreview) => {
  if (editingDescription.value && !isPreview) {
    nextTick(() => {
      descriptionTextarea.value?.focus()
      autosizeDescriptionTextarea()
    })
  }
})

function handleIntegrityFeedback(integrity?: SprintIntegrityDiagnostics | null) {
  if (!integrity) return
  const autoCleanup = integrity.auto_cleanup
  if (autoCleanup?.removed_references) {
    showToast(
      `Automatically cleaned ${autoCleanup.removed_references} dangling sprint reference${autoCleanup.removed_references === 1 ? '' : 's'}.`,
    )
  }
  if (Array.isArray(integrity.missing_sprints) && integrity.missing_sprints.length) {
    const ids = integrity.missing_sprints.map((id) => `#${id}`).join(', ')
    showToast(`Missing sprint IDs still detected: ${ids}`)
  }
}

watch(
  dialogSprintOptions,
  (options) => {
    if (!options.length) {
      sprintDialogSelection.value = 'active'
      return
    }
    if (!options.some((opt) => opt.value === sprintDialogSelection.value)) {
      sprintDialogSelection.value = options[0]?.value ?? ''
    }
  },
  { immediate: true },
)

watch(
  () => props.open,
  (isOpen) => {
    if (!isOpen && sprintDialogOpen.value) {
      closeSprintDialog(true)
    }
    if (!isOpen && syncDialogOpen.value) {
      closeSyncDialog(true)
    }
    if (!isOpen && removingSprintId.value !== null) {
      removingSprintId.value = null
    }
  },
)

const tagEditorRef = ref<{ openTagDialog: () => void } | null>(null)

watch(
  [() => props.open, () => props.focusSection, () => loading.value],
  ([isOpen, section, isLoading]) => {
    if (!isOpen || isLoading) return
    if (section !== 'tags') return
    nextTick(() => {
      tagEditorRef.value?.openTagDialog()
    })
  },
  { immediate: true },
)

const sprintDialogTitle = computed(() => (sprintDialogMode.value === 'add' ? 'Assign to sprint' : 'Remove from sprint'))

function openSprintDialog(mode: 'add' | 'remove') {
  if (rejectDeletedMutation('managing sprints')) return
  if (!hasSprints.value && mode === 'add') {
    showToast('No sprints available yet')
    return
  }
  if (!hasAssignedSprints.value && mode === 'remove') {
    showToast('This task is not assigned to any sprint')
    return
  }
  sprintDialogMode.value = mode
  sprintDialogSelection.value = dialogSprintOptions.value[0]?.value ?? 'active'
  sprintDialogAllowClosed.value = false
  sprintDialogOpen.value = true
}

function closeSprintDialog(force?: boolean | Event) {
  const forced = force === true
  if (sprintDialogSubmitting.value && !forced) return
  sprintDialogOpen.value = false
}

async function refreshSprintDataAfterMutation() {
  try {
    await refreshSprints(true)
  } catch (refreshError) {
    console.warn('Failed to refresh sprints', refreshError)
    showToast('Updated sprint list may be stale; refresh later.')
  }
  try {
    await reloadTask()
  } catch (reloadError) {
    console.warn('Failed to reload task after sprint update', reloadError)
    showToast('Task view may be out of date; refresh to confirm changes.')
  }
}

function parseSprintToken(token: string): number | string | undefined {
  const trimmed = (token || '').trim()
  if (!trimmed || trimmed === 'active' || trimmed === 'auto') return undefined
  if (trimmed === 'next') return 'next'
  if (trimmed === 'previous' || trimmed === 'prev') return 'previous'
  const numeric = Number(trimmed)
  if (Number.isInteger(numeric) && numeric > 0) return numeric
  return trimmed
}

async function submitSprintDialog() {
  if (sprintDialogSubmitting.value) return
  // DEV-104: in create mode the membership lives on the draft only — no
  // sprint API calls before the task exists. The atomic create payload
  // carries the final ids and the server rejects missing ones.
  if (mode.value === 'create') {
    submitSprintDialogDraft()
    return
  }
  const taskId = form.id || props.taskId
  if (!taskId || taskId === 'new') {
    showToast('Save the task before managing sprints')
    return
  }
  sprintDialogSubmitting.value = true
  try {
    const payload: Record<string, unknown> = {
      tasks: [taskId],
      cleanup_missing: true,
    }
    const sprintRef = parseSprintToken(sprintDialogSelection.value)
    if (sprintRef !== undefined) payload.sprint = sprintRef
    if (sprintDialogMode.value === 'add' && sprintDialogAllowClosed.value) {
      payload.allow_closed = true
    }
    const response =
      sprintDialogMode.value === 'add' ? await api.sprintAdd(payload as any) : await api.sprintRemove(payload as any)
    const changed = response.modified.length
    const replacedCount = Array.isArray(response.replaced) ? response.replaced.length : 0
    const cleanedReferences = response.integrity?.auto_cleanup?.removed_references ?? 0
    if (changed) {
      const verb = sprintDialogMode.value === 'add' ? 'Assigned' : 'Removed'
      const preposition = sprintDialogMode.value === 'add' ? 'to' : 'from'
      const label = response.sprint_label || `Sprint #${response.sprint_id}`
      showToast(`${verb} ${changed} task${changed === 1 ? '' : 's'} ${preposition} ${label}`)
    } else {
      showToast(sprintDialogMode.value === 'add' ? 'No changes applied' : 'No sprints removed')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    if (messages.length) {
      messages.forEach((message) => showToast(message))
    } else if (sprintDialogMode.value === 'add' && Array.isArray(response.replaced) && response.replaced.length) {
      response.replaced.forEach((entry) => {
        if (!entry?.previous?.length) return
        const prev = entry.previous.map((id) => `#${id}`).join(', ')
        showToast(`${entry.task_id} moved from ${prev}`)
      })
    }
    handleIntegrityFeedback(response.integrity)
    // DEV-102: reconcile the sprint list and task only after a real change
    // (modified tasks, reassignments, or integrity cleanup). A pure no-op
    // response leaves both untouched — no refresh requests.
    if (changed > 0 || replacedCount > 0 || cleanedReferences > 0) {
      await refreshSprintDataAfterMutation()
    }
    closeSprintDialog(true)
  } catch (error: any) {
    showToast(
      error?.message || (sprintDialogMode.value === 'add' ? 'Failed to assign sprint' : 'Failed to remove sprint'),
    )
  } finally {
    sprintDialogSubmitting.value = false
  }
}

/** DEV-104: local draft mutation for create mode; never sends a request. */
function submitSprintDialogDraft() {
  const sprintRef = parseSprintToken(sprintDialogSelection.value)
  if (sprintDialogMode.value === 'add') {
    if (typeof sprintRef !== 'number' || !Number.isInteger(sprintRef) || sprintRef <= 0) {
      showToast('Select a sprint')
      return
    }
    const state = (sprintLookup.value[sprintRef]?.state || '').toLowerCase()
    if (state === 'complete' && !sprintDialogAllowClosed.value) {
      showToast('Enable "Allow assigning to closed sprints" to add this sprint.')
      return
    }
    if (form.sprints.includes(sprintRef)) {
      showToast('Sprint already selected')
      closeSprintDialog(true)
      return
    }
    form.sprints = [...form.sprints, sprintRef]
  } else if (typeof sprintRef === 'number') {
    form.sprints = form.sprints.filter((id) => id !== sprintRef)
  }
  closeSprintDialog(true)
}

function formatRemote(remote: SyncRemoteConfig): string {
  if (!remote) return 'Remote'
  if (remote.provider === 'jira') {
    return remote.project ? `Jira · ${remote.project}` : 'Jira'
  }
  if (remote.provider === 'github') {
    return remote.repo ? `GitHub · ${remote.repo}` : 'GitHub'
  }
  return String(remote.provider)
}

function openSyncDialog() {
  if (rejectDeletedMutation('syncing')) return
  if (!hasSyncRemotes.value) {
    showToast('No sync remotes configured for this project')
    return
  }
  syncDialogAction.value = 'pull'
  syncDialogRemote.value = syncRemoteEntries.value[0]?.name ?? ''
  syncDialogWriteReport.value = syncConfigInspect.value?.effective?.sync_write_reports ?? true
  syncDialogOpen.value = true
}

function closeSyncDialog(force?: boolean | Event) {
  const forced = force === true
  if (syncDialogSubmitting.value && !forced) return
  syncDialogOpen.value = false
}

async function submitSyncDialog() {
  if (syncDialogSubmitting.value) return
  const id = panelTaskId.value
  if (!id) {
    showToast('Save the task before syncing')
    return
  }
  const remote = syncDialogRemote.value.trim()
  if (!remote) {
    showToast('Select a remote before syncing')
    return
  }
  syncDialogSubmitting.value = true
  try {
    const request: SyncRequest = {
      remote,
      project: projectPrefixOfTaskId(id) ?? undefined,
      task_id: id,
      include_report: true,
      write_report: syncDialogWriteReport.value,
    }
    const response = syncDialogAction.value === 'pull' ? await api.syncPull(request) : await api.syncPush(request)
    const summary = response.summary
    if (summary) {
      showToast(
        `Sync ${syncDialogAction.value} complete: ${summary.created} created, ${summary.updated} updated, ${summary.skipped} skipped, ${summary.failed} failed`,
      )
    } else {
      showToast(`Sync ${syncDialogAction.value} complete`)
    }
    await reloadTask()
    closeSyncDialog(true)
  } catch (error: any) {
    showToast(error?.message || `Failed to ${syncDialogAction.value} sync`)
  } finally {
    syncDialogSubmitting.value = false
  }
}

async function removeSprintChip(sprintId: number) {
  if (removingSprintId.value !== null) return
  if (rejectDeletedMutation('managing sprints')) return
  // DEV-104: draft memberships are local in create mode — no request.
  if (mode.value === 'create') {
    form.sprints = form.sprints.filter((id) => id !== sprintId)
    return
  }
  const taskId = form.id || props.taskId
  if (!taskId || taskId === 'new') {
    showToast('Save the task before managing sprints')
    return
  }
  removingSprintId.value = sprintId
  try {
    const response = await api.sprintRemove({
      tasks: [taskId],
      sprint: sprintId,
      cleanup_missing: true,
    } as any)
    const changed = response.modified.length
    const replacedCount = Array.isArray(response.replaced) ? response.replaced.length : 0
    const cleanedReferences = response.integrity?.auto_cleanup?.removed_references ?? 0
    const label = response.sprint_label || `Sprint #${response.sprint_id || sprintId}`
    if (changed) {
      showToast(`Removed ${changed} task${changed === 1 ? '' : 's'} from ${label}`)
    } else {
      showToast('No sprints removed')
    }
    const messages = Array.isArray(response.messages) ? response.messages : []
    messages.forEach((message) => showToast(message))
    handleIntegrityFeedback(response.integrity)
    // DEV-102: refresh only after a real change; a no-op removal leaves the
    // sprint list and task state untouched.
    if (changed > 0 || replacedCount > 0 || cleanedReferences > 0) {
      await refreshSprintDataAfterMutation()
    }
  } catch (error: any) {
    showToast(error?.message || 'Failed to remove sprint')
  } finally {
    removingSprintId.value = null
  }
}
</script>

<style>
.task-panel__overlay {
  position: fixed;
  inset: 0;
  background: var(--color-dialog-overlay);
  display: flex;
  justify-content: flex-end;
  z-index: var(--z-drawer);
}

.task-panel {
  width: min(640px, 100%);
  height: 100%;
  background: var(--color-bg, var(--bg));
  display: flex;
  flex-direction: column;
  position: relative;
  box-shadow: var(--shadow-dialog);
  border-left: 1px solid var(--color-border, var(--border));
}

.task-panel__header {
  padding: var(--space-4, 1rem) var(--space-4, 1rem) var(--space-3, 0.75rem);
  display: flex;
  justify-content: space-between;
  align-items: center;
  border-bottom: 1px solid var(--color-border, var(--border));
  gap: var(--space-3, 0.75rem);
}

.task-panel__title h2 {
  margin: 0;
  font-size: var(--text-lg, 1.25rem);
}

.task-panel__id {
  display: block;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__header-actions {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel__header-sync {
  display: inline-flex;
  align-items: center;
  gap: 6px;
}

.badge {
  padding: 0.2rem 0.6rem;
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  letter-spacing: 0.02em;
}

.task-panel__status-badge {
  align-self: center;
  white-space: nowrap;
}

.badge--success {
  background: color-mix(in oklab, var(--color-success) 14%, transparent);
  color: var(--color-success-strong);
}

.badge--info {
  background: color-mix(in oklab, var(--color-info) 14%, transparent);
  color: var(--color-accent-strong);
}

.badge--danger {
  background: color-mix(in oklab, var(--color-danger) 14%, transparent);
  color: var(--color-danger-strong);
}

.badge--muted {
  background: color-mix(in oklab, var(--color-muted) 16%, transparent);
  color: var(--color-fg);
}

.task-panel__body {
  flex: 1;
  overflow-y: auto;
  overflow-x: visible;
  padding: var(--space-4, 1rem);
}

.task-panel__sprints-loading {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  color: var(--color-muted);
}

.task-panel__sprint-area {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__sprint-chip-remove {
  margin-left: var(--space-1, 0.25rem);
  border: 0;
  background: transparent;
  color: inherit;
  padding: 0;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  transition: color 0.15s ease;
}

.task-panel__sprint-chip-remove .icon-glyph {
  width: 0.75rem;
  height: 0.75rem;
}

.task-panel__sprint-chip-remove:hover:not(:disabled),
.task-panel__sprint-chip-remove:focus-visible {
  color: var(--color-danger);
}

.task-panel__sprint-chip-remove:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.task-panel__sprint-field .chip-field__control {
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
}

.task-panel__sprint-field .chip-field__chip {
  padding: 0;
  background: transparent;
}

.task-panel__sprint-field .chip-field__add {
  margin-left: 0;
}

.task-panel__sprint-chip {
  display: inline-flex;
  align-items: center;
  padding: calc(var(--space-1, 0.25rem)) var(--space-2, 0.5rem);
  border-radius: var(--radius-pill);
  font-size: var(--text-xs, 0.75rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 85%, transparent);
  color: var(--color-muted);
  border: 1px solid color-mix(in oklab, var(--color-border) 70%, transparent);
}

.task-panel__sprint-chip--active {
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
  color: var(--color-accent-strong);
  border-color: color-mix(in oklab, var(--color-accent) 55%, transparent);
}

.task-panel__sprint-chip--overdue {
  background: color-mix(in oklab, var(--color-danger) 18%, transparent);
  color: var(--color-danger);
  border-color: color-mix(in oklab, var(--color-danger) 55%, transparent);
}

.task-panel__sprint-chip--complete {
  background: color-mix(in oklab, var(--color-success) 18%, transparent);
  color: var(--color-success-strong);
  border-color: color-mix(in oklab, var(--color-success) 55%, transparent);
}

.task-panel__sprint-chip--pending,
.task-panel__sprint-chip--unknown {
  background: color-mix(in oklab, var(--color-muted) 18%, transparent);
  color: var(--color-muted);
  border-color: color-mix(in oklab, var(--color-muted) 55%, transparent);
}

.task-panel__sprint-chip--missing {
  background: color-mix(in oklab, var(--color-danger) 12%, transparent);
  color: var(--color-danger);
  border-color: color-mix(in oklab, var(--color-danger) 55%, transparent);
  border-style: dashed;
}

.task-panel__sprint-warning {
  margin-top: var(--space-2, 0.5rem);
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-danger);
}

.task-panel__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

/* DEV-92: read-only shell + trash banner for deleted tasks. */
.task-panel__form-fieldset {
  border: 0;
  margin: 0;
  padding: 0;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

.task-panel__form-fieldset:disabled {
  opacity: 0.75;
}

.task-panel__deleted-badge {
  border: 1px solid color-mix(in oklab, var(--color-danger) 45%, transparent);
  background: color-mix(in oklab, var(--color-danger) 12%, transparent);
  color: var(--color-danger);
}

.task-panel__deleted-banner {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-3, 0.75rem);
  flex-wrap: wrap;
  padding: var(--space-3, 0.75rem);
  margin-bottom: var(--space-3, 0.75rem);
  border: 1px solid color-mix(in oklab, var(--color-danger) 40%, transparent);
  border-left: 3px solid var(--color-danger);
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-danger) 8%, transparent);
}

.task-panel__deleted-banner-text {
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.task-panel__deleted-banner-actions {
  display: flex;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.task-panel__group {
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-lg, 0.75rem);
  padding: var(--space-3, 0.75rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__group legend,
.task-panel__group summary {
  font-weight: 600;
  font-size: var(--text-sm, 0.875rem);
  margin-bottom: 0;
}

details.task-panel__group[open] > summary {
  margin-bottom: var(--space-2, 0.5rem);
}

details.task-panel__group:not([open]) {
  padding-bottom: 0;
}

.task-panel__row {
  display: flex;
  gap: var(--space-2, 0.5rem);
}

.task-panel__row > * {
  flex: 1;
  min-width: 0;
}

.task-panel__tags-section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__tags-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
}

.task-panel__tags-label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 500;
  color: var(--color-muted, var(--muted));
}

.task-panel__tags-list {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 6px;
}

.task-panel__tag {
  display: inline-flex;
  align-items: center;
}

.task-panel__tag-remove {
  border: 0;
  background: transparent;
  color: inherit;
  padding: 0;
  margin-left: 6px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  opacity: 0.7;
}

.task-panel__tag-remove:hover,
.task-panel__tag-remove:focus-visible {
  opacity: 1;
}

.task-panel__tag-remove .icon-glyph {
  width: 0.75rem;
  height: 0.75rem;
}

.task-panel__tag-dialog .task-panel-dialog__card {
  width: min(520px, 100%);
}

.task-panel__tag-dialog-body {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__tag-dialog-current {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__tag--recent {
  border-color: var(--color-accent, var(--accent));
}

.task-panel__tag-suggestions {
  list-style: none;
  margin: 0;
  padding: var(--space-2, 0.5rem);
  background: color-mix(in oklab, var(--color-surface) 96%, transparent);
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.5rem);
  box-shadow: var(--shadow-popover);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  max-height: 300px;
  overflow-y: auto;
}

.task-panel__tag-suggestions-item {
  margin: 0;
}

.task-panel__tag-suggestion {
  width: 100%;
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-1, 0.25rem) var(--space-3, 0.75rem);
  border: none;
  border-radius: var(--radius-sm, 0.25rem);
  background: transparent;
  color: inherit;
  font-size: var(--text-sm, 0.875rem);
  text-align: left;
  cursor: pointer;
  transition:
    background var(--duration-med) var(--ease-standard),
    color var(--duration-med) var(--ease-standard);
}

.task-panel__tag-suggestion:hover,
.task-panel__tag-suggestion:focus-visible {
  background: color-mix(in oklab, var(--color-accent) 12%, transparent);
}

.task-panel__tag-suggestion.active,
.task-panel__tag-suggestion.active:hover,
.task-panel__tag-suggestion.active:focus-visible {
  background: color-mix(in oklab, var(--color-accent) 20%, transparent);
  color: var(--color-fg, var(--fg));
}

.task-panel__tag-suggestion-label {
  display: inline-flex;
  flex-wrap: wrap;
  gap: 0;
}

.task-panel__tag-suggestion-part {
  font-weight: 400;
}

.task-panel__tag-suggestion-part--match {
  font-weight: 600;
  color: color-mix(in oklab, var(--color-accent) 70%, var(--color-fg, var(--fg)) 30%);
}

.task-panel__tag-info {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__tag-hint {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-danger-strong);
}

.task-panel__row--ownership {
  align-items: stretch;
}

.task-panel__activity {
  position: relative;
  gap: var(--space-3, 0.75rem);
}

.task-panel-dialog__overlay {
  position: fixed;
  inset: 0;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-5, 1.25rem);
  background: var(--color-dialog-overlay);
  z-index: var(--z-modal);
}

.task-panel-dialog__card {
  width: min(440px, 100%);
  max-height: calc(100vh - var(--space-6, 1.5rem));
  overflow-y: auto;
}

.task-panel__sync-dialog .task-panel-dialog__card {
  width: min(520px, 100%);
}

.task-panel__references-dialog {
  padding: var(--space-3, 0.75rem);
}

.task-panel__references-dialog .task-panel-dialog__card {
  transition:
    width var(--duration-med) var(--ease-standard),
    max-height var(--duration-med) var(--ease-standard);
  will-change: width, max-height;
}

.task-panel__references-dialog--link .task-panel-dialog__card {
  width: min(520px, 100%);
}

.task-panel__references-dialog--code .task-panel-dialog__card {
  width: min(520px, 100%);
  max-height: clamp(300px, 60vh, 520px);
}

.task-panel__references-dialog--code.task-panel__references-dialog--has-preview .task-panel-dialog__card {
  width: min(980px, 100%);
  max-height: clamp(360px, 92vh, 1200px);
}

.task-panel__references-dialog--code.task-panel__references-dialog--has-preview .task-panel__reference-snippet {
  max-height: none;
}

.task-panel-dialog__form {
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

.task-panel-dialog__header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__header h2 {
  margin: 0;
  font-size: var(--text-lg, 1.25rem);
}

.task-panel-dialog__field {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__checkbox {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel-dialog__footer {
  display: flex;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.task-panel-dialog__form--sprint .task-panel-dialog__header {
  min-width: 0;
}

.task-panel-dialog__form--sprint .task-panel-dialog__header h2 {
  min-width: 0;
  overflow-wrap: anywhere;
}

.task-panel-dialog__form--sprint .task-panel-dialog__field .input {
  width: 100%;
  min-width: 0;
}

.task-panel-dialog__form--sprint .task-panel-dialog__checkbox {
  flex-wrap: wrap;
}

.task-panel-dialog__form--sprint .task-panel-dialog__footer {
  flex-wrap: wrap;
}

.task-panel__tabs {
  display: flex;
  gap: var(--space-2, 0.5rem);
  border-bottom: 1px solid var(--color-border, var(--border));
  padding-bottom: var(--space-2, 0.5rem);
  padding-right: 2.75rem;
  margin-bottom: 0;
  flex-wrap: wrap;
}

.task-panel__tab {
  border: none;
  background: transparent;
  color: var(--color-muted, var(--muted));
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-radius: var(--radius-pill);
  font-weight: 500;
  cursor: pointer;
  transition:
    background 0.2s ease,
    color 0.2s ease;
}

.task-panel__tab:hover {
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-surface) 60%, transparent);
}

.task-panel__tab--active {
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-accent) 18%, transparent);
}

.task-panel__tab-panel {
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
}

.task-panel__tab-actions {
  position: absolute;
  top: var(--space-2, 0.5rem);
  right: var(--space-2, 0.5rem);
  z-index: var(--z-control);
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel__tab-action {
  position: static;
}

.task-panel__ownership-column {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  flex: 1;
  min-width: 0;
}

.task-panel__ownership-label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 500;
  color: var(--color-muted, var(--muted));
}

.task-panel__ownership-custom {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
}

.task-panel__ownership-custom .input {
  flex: 1;
}

.task-panel__ownership-reset {
  flex: 0 0 auto;
  white-space: nowrap;
}

textarea {
  width: 100%;
  max-width: 100%;
  font: inherit;
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  background: var(--color-surface, var(--bg));
  color: inherit;
  box-sizing: border-box;
}

.task-panel__description {
  --task-panel-description-min-height: calc(5 * 1.5em + var(--space-4, 1rem));
  display: flex;
  flex-direction: column;
}

.task-panel__description-display {
  min-height: var(--task-panel-description-min-height);
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  background: var(--color-surface, var(--bg));
  cursor: text;
  box-sizing: border-box;
}

.task-panel__description-display:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.task-panel__description-editor {
  position: relative;
}

/* Pinned inside the editor's bottom-right corner. The textarea and preview
   reserve bottom padding for it, so it never covers text or shifts layout. */
.task-panel__description-controls {
  --task-panel-description-controls-inset: var(--space-1, 0.25rem);
  position: absolute;
  right: var(--task-panel-description-controls-inset);
  bottom: var(--task-panel-description-controls-inset);
  z-index: var(--z-control);
}

.task-panel__description-body {
  min-height: var(--task-panel-description-min-height);
  display: flex;
}

.task-panel__description-body textarea {
  min-height: var(--task-panel-description-min-height);
  padding-bottom: calc(1.75rem + var(--space-2, 0.5rem));
  resize: none;
  overflow: hidden;
}

.task-panel__description-preview-toggle {
  font-size: var(--text-xs, 0.75rem);
  min-height: 0;
  height: 1.75rem;
  padding: 0 var(--space-2, 0.5rem);
  white-space: nowrap;
}

.task-panel__description-preview {
  min-height: var(--task-panel-description-min-height);
  width: 100%;
  padding: var(--space-2, 0.5rem);
  padding-bottom: calc(1.75rem + var(--space-2, 0.5rem));
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 94%, transparent);
  box-sizing: border-box;
}

.task-panel__description-preview:focus-visible {
  outline: none;
  box-shadow: var(--focus-ring);
}

.task-panel--drag-active {
  box-shadow: var(--focus-ring);
}

.task-panel__drop-overlay {
  position: absolute;
  inset: 0;
  pointer-events: none;
  display: flex;
  align-items: center;
  justify-content: center;
  padding: var(--space-4, 1rem);
  z-index: var(--z-sticky);
  background: color-mix(in oklab, var(--color-bg, var(--bg)) 35%, transparent);
  backdrop-filter: blur(1px);
}

.task-panel__drop-overlay-card {
  display: inline-flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  padding: var(--space-3, 0.75rem) var(--space-4, 1rem);
  border-radius: var(--radius-lg, 0.5rem);
  border: 1px dashed var(--color-border, var(--border));
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 88%, transparent);
  text-align: center;
  box-shadow: var(--shadow-dialog);
}

.task-panel__attachments {
  margin-top: var(--space-2, 0.5rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__attachments-header {
  display: flex;
  align-items: baseline;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__attachments-header {
  align-items: center;
  min-height: 28px;
}

.task-panel__attach-button.btn {
  gap: 4px;
  padding: 2px 8px;
  font-size: var(--text-xs, 0.75rem);
}

.task-panel__attachments-empty {
  margin: 0;
  padding: 12px;
  width: 100%;
  border: 1px dashed var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.375rem);
  background: transparent;
  font: inherit;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
  text-align: center;
  cursor: pointer;
}

.task-panel__attachments-empty:hover,
.task-panel__attachments-empty:focus-visible {
  border-color: var(--color-accent);
  color: var(--color-fg, inherit);
}

.task-panel__attachments-browse {
  color: var(--color-accent-strong);
  text-decoration: underline;
}

.task-panel__attachments-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: row;
  flex-wrap: wrap;
  gap: var(--space-2, 0.5rem);
}

.task-panel__attachment {
  max-width: 100%;
}

.task-panel__attachment-chip {
  display: flex;
  align-items: center;
  gap: var(--space-1, 0.25rem);
  padding: 2px var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  border: 1px solid var(--color-border, var(--border));
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 96%, transparent);
  min-width: 0;
}

.task-panel__attachment-link {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  text-decoration: none;
  min-width: 0;
  flex: 1;
}

.task-panel__attachment-link:hover {
  text-decoration: underline;
}

.task-panel__attachment-remove {
  flex: 0 0 auto;
}

.task-panel__attachment-remove.btn.icon-only {
  width: 1.6rem;
  height: 1.6rem;
  min-width: 0;
  min-height: 0;
}

.task-panel__attachment-remove.btn.icon-only .icon-glyph {
  font-size: 1rem;
}

.task-panel__gallery {
  list-style: none;
  padding: 0;
  margin: 0;
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  gap: var(--space-2, 0.5rem);
}

.task-panel__gallery-item {
  position: relative;
  min-width: 0;
}

.task-panel__gallery-link.task-panel__attachment-link {
  display: flex;
  flex-direction: column;
  align-items: stretch;
  gap: 4px;
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.375rem);
  overflow: hidden;
  background: var(--color-surface);
}

.task-panel__gallery-link:hover {
  text-decoration: none;
  border-color: var(--color-accent);
}

.task-panel__gallery-preview {
  display: block;
  height: 120px;
  overflow: hidden;
  border-bottom: 1px solid var(--color-border, var(--border));
}

.task-panel__gallery-name {
  color: var(--color-fg);
  padding: 0 8px 6px;
  font-size: var(--text-xs, 0.75rem);
}

.task-panel__gallery-actions {
  position: absolute;
  top: 4px;
  right: 4px;
  display: flex;
  gap: 4px;
  opacity: 0;
  transition: opacity 0.12s ease;
}

.task-panel__gallery-action.btn.icon-only {
  background: color-mix(in oklab, var(--color-bg) 85%, transparent);
}

.task-panel__gallery-item:hover .task-panel__gallery-actions,
.task-panel__gallery-item:focus-within .task-panel__gallery-actions {
  opacity: 1;
}

@media (hover: none) {
  .task-panel__gallery-actions {
    opacity: 1;
  }
}

.task-panel__cover-badge {
  position: absolute;
  top: 6px;
  left: 6px;
  padding: 1px 6px;
  border-radius: var(--radius-pill);
  font-size: 11px;
  font-weight: 600;
  color: var(--color-fg);
  background: color-mix(in oklab, var(--color-bg) 85%, transparent);
  pointer-events: none;
}

.task-panel__gallery-item[draggable='true'] .task-panel__gallery-link {
  cursor: grab;
}

.task-panel__gallery-item--drop .task-panel__gallery-link,
.task-panel__attachment--drop .task-panel__attachment-chip {
  outline: 2px solid var(--color-accent);
  outline-offset: 2px;
}

.task-panel__upload-progress {
  list-style: none;
  margin: 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
  font-size: var(--text-xs, 0.75rem);
}

.task-panel__upload-row {
  display: grid;
  grid-template-columns: minmax(0, 1fr) 120px 48px;
  align-items: center;
  gap: 8px;
}

.task-panel__upload-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-panel__upload-bar {
  width: 100%;
  height: 6px;
  accent-color: var(--color-accent);
}

.task-panel__upload-state {
  text-align: right;
  color: var(--color-muted, var(--muted));
}

.task-panel__upload-row--failed .task-panel__upload-state {
  color: var(--color-danger);
}

.task-panel__attachment-icon {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1em;
  height: 1em;
}

.task-panel__attachment-thumb {
  width: 1em;
  height: 1em;
  object-fit: cover;
  border-radius: var(--radius-sm, 0.25rem);
}

.task-panel__attachment-name {
  font-size: var(--text-sm, 0.875rem);
  font-weight: 500;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.task-panel .input,
.task-panel select {
  max-width: 100%;
  box-sizing: border-box;
}

.task-panel__custom-fields {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__custom-row {
  display: flex;
  gap: var(--space-2, 0.5rem);
  align-items: center;
}

.task-panel__custom-row > *:first-child,
.task-panel__custom-row > *:nth-child(2) {
  flex: 1;
}

.task-panel__relations {
  display: grid;
  gap: var(--space-2, 0.5rem);
}

.task-panel__relation {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__relation-input {
  position: relative;
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__relation-input .input {
  width: 100%;
}

.task-panel__relation-suggest {
  position: absolute;
  top: calc(100% + 4px);
  left: 0;
  right: 0;
  list-style: none;
  margin: 0;
  padding: var(--space-1, 0.25rem) 0;
  border: 1px solid var(--color-border, var(--border));
  border-radius: var(--radius-md, 0.375rem);
  background: var(--color-bg, var(--bg));
  box-shadow: var(--shadow-sm);
  max-height: 220px;
  overflow-y: auto;
  z-index: var(--z-control-popover);
}

.task-panel__relation-suggest li {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  cursor: pointer;
}

.task-panel__relation-suggest li:hover,
.task-panel__relation-suggest li.active {
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 82%, transparent);
}

.task-panel__relation-suggest li strong {
  font-weight: 600;
}

.task-panel__relation-suggest li span {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__group-header {
  display: flex;
  justify-content: flex-end;
  align-items: center;
}

.task-panel__history-scroll {
  max-height: clamp(240px, 55vh, 520px);
  overflow-y: auto;
  width: 100%;
  padding: 0;
  box-sizing: border-box;
  display: flex;
  flex-direction: column;
  gap: var(--space-3, 0.75rem);
  scrollbar-gutter: stable;
}

.task-panel__history h4,
.task-panel__references h4 {
  margin: 0 0 var(--space-2, 0.5rem);
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, var(--muted));
  font-weight: 600;
}

.task-panel__history-list,
.task-panel__references-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__history-item {
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 96%, transparent);
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
  min-width: 0;
}

.task-panel__history-meta {
  display: flex;
  justify-content: space-between;
  gap: var(--space-2, 0.5rem);
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__history-actor {
  font-weight: 600;
}

.task-panel__history-commit {
  font-family: var(--font-mono);
  letter-spacing: 0.02em;
}

.task-panel__history-changes {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__history-change {
  display: flex;
  flex-direction: column;
  gap: var(--space-1, 0.25rem);
}

.task-panel__history-change strong {
  font-size: var(--text-xs, 0.75rem);
  text-transform: uppercase;
  letter-spacing: 0.04em;
  color: var(--color-muted, var(--muted));
}

.task-panel__history-change-values {
  display: inline-flex;
  align-items: center;
  gap: 0.35rem;
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__history-old {
  color: var(--color-muted, var(--muted));
  text-decoration: line-through;
}

.task-panel__history-new {
  font-weight: 600;
}

.task-panel__commits-list {
  list-style: none;
  padding: 0;
  margin: 0;
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__history-message {
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__history-author {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
}

.task-panel__references-list {
  gap: var(--space-2, 0.5rem);
}

.task-panel__reference-item {
  display: flex;
  align-items: flex-start;
  gap: var(--space-2, 0.5rem);
  position: relative;
  overflow: visible;
  min-width: 0;
}

.task-panel__reference-item--interactive {
  cursor: pointer;
}

.task-panel__reference-item--interactive:focus {
  outline: 2px solid color-mix(in oklab, var(--color-primary) 60%, transparent);
  outline-offset: 2px;
}

.task-panel__reference-kind {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex: 0 0 auto;
  width: 1em;
  height: 1em;
  margin-top: 0.1rem;
  color: var(--color-muted, var(--muted));
}

.task-panel__reference-text {
  font-size: var(--text-sm, 0.875rem);
  word-break: break-word;
  min-width: 0;
}

.task-panel__reference-link {
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-primary);
  text-decoration: none;
  word-break: break-word;
  min-width: 0;
}

.task-panel__reference-link:hover {
  text-decoration: underline;
}

.task-panel__reference-remove {
  margin-left: auto;
  flex: 0 0 auto;
}

.task-panel__reference-preview {
  position: fixed;
  top: 0;
  left: 0;
  z-index: 90;
  width: max-content;
  max-width: min(980px, calc(100vw - 2rem));
  max-height: calc(100vh - 2rem);
  overflow: auto;
  scrollbar-gutter: stable;
  padding: var(--space-4, 1rem);
  border-radius: var(--radius-lg, 0.75rem);
  border: 1px solid var(--color-border, var(--border));
  background: var(--color-surface, var(--bg));
  box-shadow: var(--shadow-dialog);
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.task-panel__reference-preview .task-panel__reference-snippet {
  max-height: none;
  overflow: visible;
}

.task-panel__reference-meta {
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted, var(--muted));
  display: flex;
  gap: var(--space-1, 0.25rem);
  align-items: baseline;
}

.task-panel__reference-snippet {
  max-height: 420px;
  overflow: auto;
  border-radius: var(--radius-md, 0.375rem);
  background: color-mix(in oklab, var(--color-surface, var(--bg)) 88%, transparent);
  border: 1px solid color-mix(in oklab, var(--color-border, var(--border)) 60%, transparent);
  font-family: var(--font-mono);
}

.task-panel__reference-actions {
  display: flex;
  justify-content: flex-end;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
  margin-top: var(--space-2, 0.5rem);
}

.task-panel__reference-expand {
  font-size: var(--text-xs, 0.75rem);
  padding: var(--space-1, 0.25rem) var(--space-2, 0.5rem);
}

.task-panel__reference-line {
  display: grid;
  grid-template-columns: minmax(2.5rem, auto) 1fr;
  gap: var(--space-2, 0.5rem);
  padding: 0 var(--space-3, 0.75rem);
  white-space: pre;
  font-size: var(--text-xs, 0.75rem);
}

.task-panel__reference-line-number {
  color: var(--color-muted, var(--muted));
  text-align: right;
  user-select: none;
}

.task-panel__reference-line-text {
  color: var(--color-fg, var(--fg));
}

.task-panel__reference-line--highlight {
  background: color-mix(in oklab, var(--color-primary) 18%, transparent);
}

.task-panel__reference-error {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-danger);
}

.task-panel__footer {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: var(--space-3, 0.75rem);
}

.task-panel__errors {
  color: var(--color-danger-strong);
  font-size: var(--text-sm, 0.875rem);
}

.task-panel__loading {
  flex: 1;
  display: flex;
  justify-content: center;
  align-items: center;
}

.task-panel__empty-hint {
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted, var(--muted));
}

.fade-enter-active,
.fade-leave-active {
  transition: opacity var(--duration-fast) var(--ease-standard);
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
}

.task-panel-enter-from,
.task-panel-leave-to {
  opacity: 0;
  transform: translateX(40px);
}

.task-panel-enter-active,
.task-panel-leave-active {
  transition: all var(--duration-med) var(--ease-standard);
}
</style>
