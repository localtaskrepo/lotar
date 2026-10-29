<template>
  <div class="automation-builder">
    <div class="automation-builder__toolbar">
      <div class="col" style="gap: 4px">
        <h3 class="automation-builder__title">Rules</h3>
        <p class="muted automation-builder__subtitle">
          Each rule reads <strong>when</strong> something happens, <strong>if</strong> the task matches,
          <strong>then</strong> do something. Rules run top to bottom.
        </p>
      </div>
      <UiButton
        variant="primary"
        type="button"
        :disabled="loading || hasParseError || !editable"
        data-testid="automation-new-rule"
        @click="openCreateDialog"
      >
        <IconGlyph name="plus" />
        <span>New rule</span>
      </UiButton>
    </div>

    <div class="automation-builder__meta row" style="gap: 8px; align-items: center; flex-wrap: wrap">
      <span v-if="sourceLabel" class="muted">Effective source: {{ sourceLabel }}</span>
      <span v-if="sourceLabel && scopeHint" class="muted" aria-hidden="true">·</span>
      <span v-if="scopeHint" class="muted">{{ scopeHint }}</span>
    </div>

    <p v-if="error" class="field-error">{{ error }}</p>
    <p v-if="parsedDocument.parseError" class="field-error">
      YAML must be valid before the visual builder can edit it: {{ parsedDocument.parseError }}
    </p>

    <UiEmptyState
      v-if="!builderRules.length && !hasParseError"
      title="No automation rules yet"
      description="Create a rule, start from an example, or write YAML in the advanced section below."
    />

    <div v-else class="automation-builder__rule-list">
      <UiCard
        v-for="(entry, index) in builderRules"
        :key="entry.id"
        class="automation-builder__rule-card rule-card"
        :class="{ 'rule-card--off': !entry.description.enabled }"
        :data-testid="`automation-rule-${index}`"
      >
        <div class="automation-builder__rule-head">
          <div class="col" style="gap: 6px; flex: 1 1 auto; min-width: 0">
            <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap">
              <strong class="rule-name">{{ entry.description.name }}</strong>
              <span v-if="!entry.description.enabled" class="automation-builder__meta-chip">Off</span>
              <span v-if="entry.description.manual" class="automation-builder__meta-chip">Manual / cron</span>
              <span v-if="entry.rule.cooldown" class="automation-builder__meta-chip"
                >Cooldown {{ entry.rule.cooldown }}</span
              >
              <span v-if="entry.unsupportedReasons.length" class="automation-builder__warning-chip">YAML-only</span>
            </div>

            <ul class="automation-builder__lines" data-testid="automation-rule-summary">
              <li v-for="trigger in entry.description.triggers" :key="trigger.event">
                <span class="automation-builder__kw">When</span> {{ trigger.when
                }}<template v-if="entry.description.conditions.items.length"
                  >, <span class="automation-builder__kw">if</span>
                  {{
                    entry.description.conditions.items.join(
                      entry.description.conditions.join === 'any' ? ' or ' : ' and ',
                    )
                  }}</template
                >: <span class="automation-builder__kw">then</span> {{ trigger.steps.join(', ') || 'nothing' }}.
              </li>
            </ul>

            <div v-if="entry.unsupportedReasons.length" class="automation-builder__rule-note">
              This rule uses YAML features the editor does not show (for example nested or NOT conditions); edit it in
              the YAML section below.
            </div>
          </div>

          <div class="automation-builder__rule-actions">
            <label class="automation-builder__switch" :title="entry.description.enabled ? 'Turn off' : 'Turn on'">
              <input
                type="checkbox"
                :checked="entry.description.enabled"
                :disabled="!editable"
                :aria-label="`${entry.description.name} is ${entry.description.enabled ? 'on' : 'off'}`"
                :data-testid="`automation-rule-toggle-${index}`"
                @change="setEnabled(index, ($event.target as HTMLInputElement).checked)"
              />
              <span>{{ entry.description.enabled ? 'On' : 'Off' }}</span>
            </label>
            <UiButton
              v-if="entry.description.manual"
              type="button"
              :disabled="!entry.description.enabled || dirty"
              :title="dirty ? 'Save rules before running them' : 'Preview, then run this rule now'"
              :data-testid="`automation-run-${index}`"
              @click="openRunDialog(entry.description.name)"
            >
              <IconGlyph name="play" />
              <span>Run now</span>
            </UiButton>
            <UiButton
              variant="ghost"
              type="button"
              :disabled="!editable || !!entry.unsupportedReasons.length"
              :data-testid="`automation-edit-${index}`"
              @click="openEditDialog(index)"
            >
              <IconGlyph name="edit" />
              <span>Edit</span>
            </UiButton>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="!editable || index === 0"
              aria-label="Move up"
              title="Move up"
              @click="moveRule(index, -1)"
            >
              <IconGlyph name="chevron-up" />
            </UiButton>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="!editable || index === builderRules.length - 1"
              aria-label="Move down"
              title="Move down"
              @click="moveRule(index, 1)"
            >
              <IconGlyph name="chevron-down" />
            </UiButton>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="!editable"
              aria-label="Duplicate"
              title="Duplicate"
              @click="duplicateRule(index)"
            >
              <IconGlyph name="file" />
            </UiButton>
            <UiButton
              variant="ghost"
              icon-only
              type="button"
              :disabled="!editable"
              aria-label="Delete"
              title="Delete"
              @click="deleteRule(index)"
            >
              <IconGlyph name="trash" />
            </UiButton>
          </div>
        </div>
      </UiCard>
    </div>

    <details class="automation-builder__advanced" :open="hasParseError">
      <summary>Advanced YAML and rule engine settings</summary>
      <div class="automation-builder__advanced-body">
        <div class="field-grid">
          <label class="automation-builder__max-iterations field">
            <span class="field-label">Max iterations</span>
            <UiInput
              :model-value="maxIterationsInput"
              maxlength="12"
              placeholder="default"
              :disabled="loading || hasParseError"
              @update:modelValue="updateMaxIterations"
            />
            <p class="field-hint">Limits how many chained rule executions can happen for the same task.</p>
          </label>
        </div>

        <div class="field">
          <label class="field-label">
            <span>Scope YAML</span>
          </label>
          <textarea
            :value="modelValue"
            class="automation-builder__yaml"
            rows="14"
            :placeholder="loading ? 'Loading…' : 'automation:\n  rules: []'"
            @input="updateYaml(($event.target as HTMLTextAreaElement).value)"
          ></textarea>
        </div>

        <div class="field" v-if="effectiveYaml && effectiveYaml !== modelValue">
          <label class="field-label">
            <span>Effective rules (read-only)</span>
          </label>
          <textarea
            class="automation-builder__yaml automation-builder__yaml--readonly"
            rows="10"
            :value="effectiveYaml"
            readonly
          ></textarea>
        </div>
      </div>
    </details>

    <AutomationRuleDialog
      :open="dialogOpen"
      :rule="editingRule"
      :statuses="availableStatuses"
      :priorities="availablePriorities"
      :types="availableTypes"
      :tags="availableTags"
      :people="availableMembers"
      :custom-fields="availableCustomFields"
      :project="project"
      @close="dialogOpen = false"
      @save="commitRule"
    />

    <UiModal
      :open="runDialog.open"
      size="md"
      aria-label="Run rule now"
      :aria-labelledby="runTitleId"
      @close="closeRunDialog"
    >
      <div class="automation-builder__run" data-testid="automation-run-dialog">
        <header class="automation-builder__run-header">
          <h2 :id="runTitleId">Run “{{ runDialog.rule }}” now</h2>
          <UiButton variant="ghost" icon-only type="button" aria-label="Close dialog" @click="closeRunDialog">
            <IconGlyph name="close" />
          </UiButton>
        </header>
        <p v-if="runDialog.loading" class="muted">Checking which tasks match…</p>
        <p v-else-if="runDialog.error" class="field-error" role="alert">{{ runDialog.error }}</p>
        <template v-else-if="runDialog.result">
          <p v-if="!runDialog.result.entries.length" class="muted" data-testid="automation-run-empty">
            None of the {{ runDialog.result.checked }} tasks match right now; nothing to do.
          </p>
          <template v-else>
            <p>
              {{ runDialog.result.dry_run ? 'It would change' : 'Changed' }}
              {{ runDialog.result.entries.length }} task(s):
            </p>
            <ul class="automation-builder__run-list" data-testid="automation-run-preview">
              <li v-for="entry in runDialog.result.entries" :key="`${entry.rule}-${entry.task}`">
                <strong>{{ entry.task }}</strong>
                <span class="muted">— {{ entry.actions.map((action) => action.description).join('; ') }}</span>
                <span v-if="entry.error" class="field-error"> {{ entry.error }}</span>
              </li>
            </ul>
          </template>
        </template>
        <p class="field-hint">
          To run it on a schedule, call
          <code>lotar automation run "{{ runDialog.rule }}"</code> from cron or CI.
        </p>
        <footer class="automation-builder__run-footer">
          <UiButton type="button" @click="closeRunDialog">{{
            runDialog.result?.dry_run === false ? 'Close' : 'Cancel'
          }}</UiButton>
          <UiButton
            v-if="runDialog.result?.dry_run && runDialog.result.entries.length"
            variant="primary"
            type="button"
            :disabled="runDialog.loading"
            data-testid="automation-run-confirm"
            @click="confirmRun"
          >
            Run on {{ runDialog.result.entries.length }} task(s)
          </UiButton>
        </footer>
      </div>
    </UiModal>
  </div>
</template>

<script setup lang="ts">
import { computed, reactive, ref, useId } from 'vue'
import { api } from '../api/client'
import type { AutomationRunResponse } from '../api/types'
import {
  buildAutomationYamlDocument,
  getEditableRuleState,
  parseAutomationYamlDocument,
  type AutomationRecord,
} from '../utils/automationRules'
import { describeAutomationRule } from '../utils/automationText'
import AutomationRuleDialog from './automation/AutomationRuleDialog.vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiCard from './UiCard.vue'
import UiEmptyState from './UiEmptyState.vue'
import UiInput from './UiInput.vue'
import UiModal from './UiModal.vue'

const props = withDefaults(
  defineProps<{
    modelValue: string
    effectiveYaml?: string
    loading?: boolean
    error?: string | null
    sourceLabel?: string
    scopeHint?: string
    editable?: boolean
    /** Unsaved changes: "Run now" runs the saved rules, so it waits for a save. */
    dirty?: boolean
    project?: string
    availableStatuses?: string[]
    availablePriorities?: string[]
    availableTypes?: string[]
    availableTags?: string[]
    availableMembers?: string[]
    availableCustomFields?: string[]
  }>(),
  {
    effectiveYaml: '',
    loading: false,
    error: null,
    sourceLabel: '',
    scopeHint: '',
    editable: true,
    dirty: false,
    project: '',
    availableStatuses: () => [],
    availablePriorities: () => [],
    availableTypes: () => [],
    availableTags: () => [],
    availableMembers: () => [],
    availableCustomFields: () => [],
  },
)

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
  (e: 'ran', result: AutomationRunResponse): void
}>()

const parsedDocument = computed(() => parseAutomationYamlDocument(props.modelValue))
const hasParseError = computed(() => Boolean(parsedDocument.value.parseError))
const maxIterationsInput = computed(() => parsedDocument.value.maxIterations)

const builderRules = computed(() =>
  parsedDocument.value.rules.map((rule, index) => ({
    id: `${index}-${typeof rule.name === 'string' ? rule.name : 'rule'}`,
    rule,
    description: describeAutomationRule(rule, index),
    unsupportedReasons: getEditableRuleState(rule).unsupportedReasons,
  })),
)

const dialogOpen = ref(false)
const runTitleId = `automation-run-title-${useId()}`
const editingIndex = ref<number | null>(null)
const editingRule = ref<AutomationRecord | null>(null)

function write(rules: AutomationRecord[]) {
  emit('update:modelValue', buildAutomationYamlDocument(rules, parsedDocument.value.maxIterations))
}

function updateYaml(value: string) {
  emit('update:modelValue', value)
}

function updateMaxIterations(value: string) {
  emit('update:modelValue', buildAutomationYamlDocument(parsedDocument.value.rules, value))
}

function openCreateDialog() {
  editingIndex.value = null
  editingRule.value = null
  dialogOpen.value = true
}

function openEditDialog(index: number) {
  const rule = parsedDocument.value.rules[index]
  if (!rule) return
  editingIndex.value = index
  editingRule.value = rule
  dialogOpen.value = true
}

function commitRule(rule: AutomationRecord) {
  const rules = [...parsedDocument.value.rules]
  if (editingIndex.value === null) rules.push(rule)
  else rules.splice(editingIndex.value, 1, rule)
  write(rules)
  dialogOpen.value = false
}

function setEnabled(index: number, enabled: boolean) {
  const rules = [...parsedDocument.value.rules]
  const rule = rules[index]
  if (!rule) return
  const next: AutomationRecord = {}
  // Keep `enabled` next to the name for readable YAML.
  for (const [key, value] of Object.entries(rule)) {
    if (key === 'enabled') continue
    next[key] = value
    if (key === 'name' && !enabled) next.enabled = false
  }
  if (!enabled && !('enabled' in next)) next.enabled = false
  rules[index] = next
  write(rules)
}

function deleteRule(index: number) {
  const rules = [...parsedDocument.value.rules]
  rules.splice(index, 1)
  write(rules)
}

function duplicateRule(index: number) {
  const rules = [...parsedDocument.value.rules]
  const clone = JSON.parse(JSON.stringify(rules[index])) as AutomationRecord
  if (typeof clone.name === 'string') clone.name = `${clone.name} (copy)`
  rules.splice(index + 1, 0, clone)
  write(rules)
}

function moveRule(index: number, direction: number) {
  const target = index + direction
  const rules = [...parsedDocument.value.rules]
  if (target < 0 || target >= rules.length) return
  const [rule] = rules.splice(index, 1)
  if (rule) rules.splice(target, 0, rule)
  write(rules)
}

// ── Run now ─────────────────────────────────────────────────────────────────

const runDialog = reactive({
  open: false,
  rule: '',
  loading: false,
  error: '',
  result: null as AutomationRunResponse | null,
})

function runRequest(dryRun: boolean) {
  return api.runAutomation({
    rules: [runDialog.rule],
    query: props.project ? `project=${props.project}` : undefined,
    dry_run: dryRun,
  })
}

async function openRunDialog(rule: string) {
  Object.assign(runDialog, { open: true, rule, loading: true, error: '', result: null })
  try {
    runDialog.result = await runRequest(true)
  } catch (error) {
    runDialog.error = (error as Error | undefined)?.message || 'Could not check the rule'
  } finally {
    runDialog.loading = false
  }
}

async function confirmRun() {
  runDialog.loading = true
  runDialog.error = ''
  try {
    const result = await runRequest(false)
    runDialog.result = result
    emit('ran', result)
  } catch (error) {
    runDialog.error = (error as Error | undefined)?.message || 'The run failed'
  } finally {
    runDialog.loading = false
  }
}

function closeRunDialog() {
  runDialog.open = false
}
</script>

<style scoped>
/* Field layout wherever the editor is shown. The Config page styles these
   classes too, but the Automations page does not. */
.field {
  display: flex;
  flex-direction: column;
  gap: 6px;
  min-width: 0;
}

.field-grid {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 18px 20px;
  align-items: start;
}

.field-label {
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 8px;
  min-height: 20px;
  font-size: var(--text-sm);
  font-weight: 600;
  color: var(--color-fg);
}

.field-hint {
  margin: 0;
  font-size: 12px;
  line-height: 1.45;
  color: var(--color-muted);
}

.field :deep(input.input),
.field :deep(select.input) {
  box-sizing: border-box;
  width: 100%;
}

@media (max-width: 640px) {
  .field-grid {
    grid-template-columns: minmax(0, 1fr);
  }
}

.automation-builder {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.automation-builder__toolbar {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
  flex-wrap: wrap;
}

.automation-builder__title {
  margin: 0;
  font-size: 1rem;
}

.automation-builder__subtitle {
  margin: 0;
  max-width: 48rem;
}

.automation-builder__meta {
  font-size: 13px;
}

.automation-builder__rule-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.automation-builder__rule-card {
  padding: 14px 16px;
}

.rule-card--off {
  opacity: 0.65;
}

.automation-builder__rule-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.automation-builder__rule-actions {
  flex: 0 0 auto;
  display: flex;
  gap: 4px;
  flex-wrap: nowrap;
  align-items: center;
  justify-content: flex-end;
}

.automation-builder__switch {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  margin-right: 6px;
  font-size: var(--text-sm, 0.875rem);
}

.automation-builder__lines {
  margin: 0;
  padding: 0;
  list-style: none;
  display: flex;
  flex-direction: column;
  gap: 2px;
  font-size: 14px;
  line-height: 1.5;
}

.automation-builder__kw {
  font-weight: 600;
}

.automation-builder__rule-note {
  padding: 8px 12px;
  border-radius: var(--radius-lg);
  background: color-mix(in srgb, var(--color-warning) 10%, transparent);
  color: var(--color-muted);
  font-size: 13px;
}

.automation-builder__meta-chip,
.automation-builder__warning-chip {
  display: inline-flex;
  align-items: center;
  padding: 2px 8px;
  border-radius: var(--radius-pill);
  font-size: 12px;
  border: 1px solid var(--color-border);
  background: color-mix(in oklab, var(--color-fg) 3%, transparent);
}

.automation-builder__warning-chip {
  background: color-mix(in srgb, var(--color-warning) 12%, transparent);
  border-color: color-mix(in srgb, var(--color-warning) 36%, var(--color-border));
}

.automation-builder__advanced {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-xl);
  background: color-mix(in oklab, var(--color-fg) 2%, transparent);
}

.automation-builder__advanced summary {
  cursor: pointer;
  padding: 14px 16px;
  font-weight: 600;
}

.automation-builder__advanced-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
  padding: 0 16px 16px;
}

.automation-builder__yaml {
  width: 100%;
  border: 1px solid var(--color-border);
  border-radius: var(--radius-lg);
  background: var(--color-surface);
  padding: 12px 14px;
  font: inherit;
  font-family: var(--font-mono);
  font-size: 13px;
  resize: vertical;
}

.automation-builder__yaml--readonly {
  opacity: 0.85;
}

.automation-builder__run {
  display: flex;
  flex-direction: column;
  gap: 10px;
  padding: 16px 20px;
}

.automation-builder__run-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.automation-builder__run-header h2 {
  margin: 0;
  font-size: var(--text-lg, 1.2rem);
}

.automation-builder__run-list {
  margin: 0;
  padding-left: 1.2rem;
  max-height: 40vh;
  overflow-y: auto;
  font-size: var(--text-sm, 0.875rem);
}

.automation-builder__run-footer {
  display: flex;
  justify-content: flex-end;
  gap: 8px;
}

@media (max-width: 720px) {
  .automation-builder__rule-head {
    flex-direction: column;
  }

  .automation-builder__rule-actions {
    justify-content: flex-start;
    flex-wrap: wrap;
  }
}
</style>
