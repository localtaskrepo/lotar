<template>
  <UiModal :open="open" size="xl" :aria-label="title" :aria-labelledby="titleId" @close="emit('close')">
    <form class="rule-dialog" data-testid="automation-rule-dialog" @submit.prevent="save">
      <header class="rule-dialog__header">
        <h2 :id="titleId">{{ title }}</h2>
        <UiButton
          variant="ghost"
          icon-only
          type="button"
          aria-label="Close dialog"
          title="Close"
          @click="emit('close')"
        >
          <IconGlyph name="close" />
        </UiButton>
      </header>

      <div class="rule-dialog__body">
        <section v-if="isNew" class="rule-dialog__recipes" aria-label="Start from an example">
          <span class="rule-dialog__label">Start from</span>
          <div class="rule-dialog__chips">
            <button
              v-for="recipe in recipes"
              :key="recipe.id"
              type="button"
              class="rule-dialog__chip"
              :class="{ 'is-active': recipeId === recipe.id }"
              :title="recipe.description"
              :data-testid="`automation-recipe-${recipe.id}`"
              @click="applyRecipe(recipe)"
            >
              {{ recipe.label }}
            </button>
          </div>
        </section>

        <div class="rule-dialog__name-row">
          <label class="rule-dialog__field rule-dialog__name">
            <span class="rule-dialog__label">Name</span>
            <input
              v-model="draft.name"
              class="input"
              placeholder="e.g. Hand back after review"
              data-testid="automation-rule-name"
            />
          </label>
          <label class="rule-dialog__switch">
            <input v-model="draft.enabled" type="checkbox" data-testid="automation-rule-enabled" />
            <span>{{ draft.enabled ? 'On' : 'Off' }}</span>
          </label>
        </div>

        <!-- WHEN -->
        <section class="rule-dialog__section" aria-labelledby="rule-when">
          <h3 id="rule-when" class="rule-dialog__section-title"><span class="rule-dialog__keyword">When</span></h3>
          <div class="rule-dialog__chips" role="group" aria-label="Triggers">
            <button
              v-for="option in triggerOptions"
              :key="option.event"
              type="button"
              class="rule-dialog__chip"
              :class="{ 'is-active': isTriggerOn(option.event) }"
              :aria-pressed="isTriggerOn(option.event)"
              :title="option.help"
              :data-testid="`automation-trigger-${option.event}`"
              @click="toggleTrigger(option.event)"
            >
              {{ option.label }}
            </button>
          </div>
          <p v-if="isTriggerOn('manual')" class="rule-dialog__hint">
            Manual rules never run by themselves. Start them with <strong>Run now</strong> in the rule list, or from
            cron with <code>lotar automation run "{{ draft.name || 'Rule name' }}"</code>. Conditions are checked
            against every task as it is at that moment.
          </p>
          <p v-if="!activeEvents.length" class="rule-dialog__hint">Pick at least one trigger.</p>
        </section>

        <!-- IF -->
        <section class="rule-dialog__section" aria-labelledby="rule-if">
          <h3 id="rule-if" class="rule-dialog__section-title">
            <span class="rule-dialog__keyword">If</span>
            <template v-if="draft.conditions.length + draft.changeConditions.length > 1">
              <select
                v-model="draft.matchMode"
                class="input rule-dialog__inline-select"
                aria-label="Combine conditions"
              >
                <option value="all">all of these are true</option>
                <option value="any">any of these is true</option>
              </select>
            </template>
            <span v-else-if="!draft.conditions.length && !draft.changeConditions.length" class="muted"
              >any task (no conditions)</span
            >
          </h3>

          <div
            v-for="row in draft.conditions"
            :key="row.id"
            class="rule-dialog__row"
            data-testid="automation-condition"
          >
            <select
              class="input rule-dialog__field-select"
              :value="conditionFieldValue(row)"
              aria-label="Field"
              @change="setConditionField(row, ($event.target as HTMLSelectElement).value)"
            >
              <option v-for="option in conditionFields" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
            <input
              v-if="row.scope === 'custom_field' || conditionFieldValue(row) === 'other'"
              v-model="row.customFieldKey"
              class="input rule-dialog__key-input"
              :placeholder="row.scope === 'custom_field' ? 'Field name' : 'YAML field'"
              :list="row.scope === 'custom_field' ? 'automation-custom-fields' : undefined"
              aria-label="Field name"
              @input="row.scope === 'field' && (row.field = row.customFieldKey)"
            />
            <select
              class="input rule-dialog__op-select"
              :value="operatorValue(row)"
              aria-label="Comparison"
              @change="setOperator(row, ($event.target as HTMLSelectElement).value)"
            >
              <option v-for="option in operatorsFor(row)" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
            <input
              v-if="row.operator !== 'exists'"
              v-model="row.value"
              class="input rule-dialog__value-input"
              :list="valueListFor(row.field, 'condition')"
              :placeholder="valuePlaceholder(row)"
              aria-label="Value"
              data-testid="automation-condition-value"
            />
            <button type="button" class="rule-dialog__remove" title="Remove condition" @click="removeCondition(row.id)">
              <IconGlyph name="close" />
            </button>
          </div>

          <div
            v-for="row in draft.changeConditions"
            :key="row.id"
            class="rule-dialog__row"
            data-testid="automation-change"
          >
            <select v-model="row.field" class="input rule-dialog__field-select" aria-label="Field that changes">
              <option v-for="option in changeFields" :key="option.value" :value="option.value">
                {{ option.label }}
              </option>
            </select>
            <span class="rule-dialog__word">changes from</span>
            <input
              v-model="row.from"
              class="input rule-dialog__value-input"
              :list="valueListFor(row.field, 'condition')"
              placeholder="anything"
              aria-label="From"
            />
            <span class="rule-dialog__word">to</span>
            <input
              v-model="row.to"
              class="input rule-dialog__value-input"
              :list="valueListFor(row.field, 'condition')"
              placeholder="anything"
              aria-label="To"
            />
            <button type="button" class="rule-dialog__remove" title="Remove condition" @click="removeChange(row.id)">
              <IconGlyph name="close" />
            </button>
          </div>

          <div class="rule-dialog__add-row">
            <button type="button" class="rule-dialog__add" data-testid="automation-add-condition" @click="addCondition">
              <IconGlyph name="plus" /> Condition
            </button>
            <button
              v-if="hasChangeTrigger"
              type="button"
              class="rule-dialog__add"
              data-testid="automation-add-change"
              @click="addChange"
            >
              <IconGlyph name="plus" /> “Changes from … to …”
            </button>
          </div>
        </section>

        <!-- THEN -->
        <section class="rule-dialog__section" aria-labelledby="rule-then">
          <h3 id="rule-then" class="rule-dialog__section-title"><span class="rule-dialog__keyword">Then</span></h3>
          <p v-if="!activeEvents.length" class="rule-dialog__hint">Pick a trigger first.</p>

          <div
            v-for="action in activeActions"
            :key="action.event"
            class="rule-dialog__event-block"
            :data-testid="`automation-actions-${action.event}`"
          >
            <p v-if="activeActions.length > 1" class="rule-dialog__event-title">
              When {{ triggerText(action.event) }}:
            </p>

            <!-- set fields -->
            <div v-for="row in action.setFields" :key="row.id" class="rule-dialog__row" data-testid="automation-action">
              <span class="rule-dialog__word">Set</span>
              <select
                class="input rule-dialog__field-select"
                :value="row.scope === 'custom_field' ? 'custom_field' : row.field"
                aria-label="Field to set"
                @change="setSetField(row, ($event.target as HTMLSelectElement).value)"
              >
                <option v-for="option in setFields" :key="option.value" :value="option.value">
                  {{ option.label }}
                </option>
              </select>
              <input
                v-if="row.scope === 'custom_field'"
                v-model="row.customFieldKey"
                class="input rule-dialog__key-input"
                placeholder="Field name"
                list="automation-custom-fields"
                aria-label="Custom field name"
              />
              <span class="rule-dialog__word">to</span>
              <AutomationTemplateField
                v-if="isTextField(row)"
                v-model="row.value"
                :label="`Value for ${row.field}`"
                :variables="variables"
                :multiline="row.field === 'description'"
                :preview-task="previewTask"
                :testid="`automation-set-${row.field}`"
              />
              <input
                v-else
                v-model="row.value"
                class="input rule-dialog__value-input"
                :list="valueListFor(row.field, 'set')"
                :placeholder="setPlaceholder(row.field)"
                :aria-label="`Value for ${row.field}`"
                :data-testid="`automation-set-${row.field}`"
              />
              <button
                type="button"
                class="rule-dialog__remove"
                title="Remove action"
                @click="removeSet(action, row.id)"
              >
                <IconGlyph name="close" />
              </button>
            </div>

            <!-- add / remove lists -->
            <template v-for="kind in ['add', 'remove'] as const" :key="kind">
              <div
                v-for="row in kind === 'add' ? action.addFields : action.removeFields"
                :key="row.id"
                class="rule-dialog__row"
                data-testid="automation-action"
              >
                <span class="rule-dialog__word">{{ kind === 'add' ? 'Add' : 'Remove' }}</span>
                <select v-model="row.field" class="input rule-dialog__field-select" :aria-label="`${kind} what`">
                  <option v-for="option in listFields" :key="option.value" :value="option.value">
                    {{ option.label }}
                  </option>
                </select>
                <input
                  v-model="row.value"
                  class="input rule-dialog__value-input"
                  :list="row.field === 'tags' || row.field === 'labels' ? 'automation-tag-values' : undefined"
                  :placeholder="listPlaceholder(row.field)"
                  :aria-label="`Values to ${kind}`"
                  :data-testid="`automation-${kind}-${row.field}`"
                />
                <button
                  type="button"
                  class="rule-dialog__remove"
                  title="Remove action"
                  @click="removeList(action, kind, row.id)"
                >
                  <IconGlyph name="close" />
                </button>
              </div>
            </template>

            <!-- comment -->
            <div
              v-if="shows(action, 'comment')"
              class="rule-dialog__row rule-dialog__row--top"
              data-testid="automation-action"
            >
              <span class="rule-dialog__word">Comment</span>
              <AutomationTemplateField
                v-model="action.comment"
                label="Comment"
                :variables="variables"
                multiline
                placeholder="e.g. Ready for review: ${{ticket.title}}"
                :preview-task="previewTask"
                testid="automation-comment"
              />
              <button
                type="button"
                class="rule-dialog__remove"
                title="Remove action"
                @click="clearExtra(action, 'comment')"
              >
                <IconGlyph name="close" />
              </button>
            </div>

            <!-- run -->
            <div
              v-if="shows(action, 'run')"
              class="rule-dialog__row rule-dialog__row--top"
              data-testid="automation-action"
            >
              <span class="rule-dialog__word">Run</span>
              <div class="rule-dialog__stack">
                <AutomationTemplateField
                  v-if="action.runMode !== 'command'"
                  v-model="action.runShell"
                  label="Shell command"
                  :variables="variables"
                  placeholder="e.g. ./scripts/notify.sh ${{ticket.id}}"
                  :preview-task="previewTask"
                  testid="automation-run-shell"
                />
                <template v-else>
                  <input
                    v-model="action.runCommand"
                    class="input"
                    placeholder="Program, e.g. lotar"
                    aria-label="Program"
                  />
                  <input
                    v-model="action.runArgs"
                    class="input"
                    placeholder="Arguments, comma separated"
                    aria-label="Arguments"
                  />
                  <input
                    v-model="action.runCwd"
                    class="input"
                    placeholder="Working folder (optional)"
                    aria-label="Working folder"
                  />
                </template>
                <div class="rule-dialog__inline">
                  <label class="rule-dialog__check">
                    <input
                      type="checkbox"
                      :checked="action.runMode === 'command'"
                      @change="action.runMode = ($event.target as HTMLInputElement).checked ? 'command' : 'shell'"
                    />
                    Program and arguments instead of a shell line
                  </label>
                  <label class="rule-dialog__check">
                    <input v-model="action.runWait" type="checkbox" /> Wait for it
                  </label>
                  <label class="rule-dialog__check">
                    <input v-model="action.runIgnoreFailure" type="checkbox" /> Ignore failures
                  </label>
                </div>
                <p class="rule-dialog__hint">
                  Runs on the machine where LoTaR handles the event. Placeholders are shell-escaped.
                </p>
              </div>
              <button
                type="button"
                class="rule-dialog__remove"
                title="Remove action"
                @click="clearExtra(action, 'run')"
              >
                <IconGlyph name="close" />
              </button>
            </div>

            <!-- agent -->
            <div
              v-if="shows(action, 'agent')"
              class="rule-dialog__row rule-dialog__row--top"
              data-testid="automation-action"
            >
              <span class="rule-dialog__word">Agent</span>
              <div class="rule-dialog__stack">
                <div class="rule-dialog__inline">
                  <input
                    v-model="action.agentModel"
                    class="input"
                    placeholder="Model (profile default)"
                    aria-label="Agent model"
                  />
                  <input
                    v-model="action.agentEffort"
                    class="input"
                    list="automation-agent-efforts"
                    placeholder="Effort (profile default)"
                    aria-label="Agent effort"
                  />
                </div>
                <AutomationTemplateField
                  v-model="action.agentPrompt"
                  label="Agent prompt"
                  :variables="variables"
                  multiline
                  placeholder="Prompt (default: work on the ticket)"
                  :preview-task="previewTask"
                  testid="automation-agent-prompt"
                />
                <p class="rule-dialog__hint">
                  Applies when this action assigns the task to an agent profile (set Assignee to an agent above).
                </p>
              </div>
              <button
                type="button"
                class="rule-dialog__remove"
                title="Remove action"
                @click="clearExtra(action, 'agent')"
              >
                <IconGlyph name="close" />
              </button>
            </div>

            <!-- archive -->
            <div v-if="shows(action, 'archive')" class="rule-dialog__row" data-testid="automation-action">
              <select v-model="action.archive" class="input rule-dialog__field-select" aria-label="Archive">
                <option value="archive">Archive the task</option>
                <option value="unarchive">Bring it back from the archive</option>
              </select>
              <span class="muted rule-dialog__hint">Runs after the other actions; archived tasks are read-only.</span>
              <button
                type="button"
                class="rule-dialog__remove"
                title="Remove action"
                @click="clearExtra(action, 'archive')"
              >
                <IconGlyph name="close" />
              </button>
            </div>

            <div class="rule-dialog__add-row">
              <select
                class="input rule-dialog__add-select"
                aria-label="Add an action"
                :data-testid="`automation-add-action-${action.event}`"
                value=""
                @change="onAddActionSelect(action, $event)"
              >
                <option value="" disabled>+ Add an action…</option>
                <option
                  v-for="option in actionMenu(action)"
                  :key="option.value"
                  :value="option.value"
                  :disabled="option.disabled"
                >
                  {{ option.label }}
                </option>
              </select>
              <span v-if="!actionHasContent(action)" class="rule-dialog__hint">Add at least one action.</span>
            </div>
          </div>
        </section>

        <!-- TRY -->
        <section class="rule-dialog__section rule-dialog__try" aria-labelledby="rule-try">
          <h3 id="rule-try" class="rule-dialog__section-title"><span class="rule-dialog__keyword">Try it</span></h3>
          <div class="rule-dialog__inline">
            <input
              v-model="previewQuery"
              class="input rule-dialog__value-input"
              list="automation-preview-tasks"
              placeholder="Task ID, e.g. DEV-12"
              aria-label="Task to try the rule on"
              data-testid="automation-try-task"
              @input="searchPreviewTasks"
              @change="loadPreviewTask"
            />
            <select
              v-model="tryEvent"
              class="input rule-dialog__inline-select"
              aria-label="Event to try"
              :disabled="!activeEvents.length"
            >
              <option v-for="event in activeEvents" :key="event" :value="event">{{ eventLabel(event) }}</option>
            </select>
            <UiButton
              type="button"
              :disabled="!previewQuery.trim() || !tryEvent || trying"
              data-testid="automation-try"
              @click="tryRule"
            >
              {{ trying ? 'Trying…' : 'Try on this task' }}
            </UiButton>
          </div>
          <p class="rule-dialog__hint">
            Shows what the rule would do to the task as it is now, without changing anything. Placeholder previews use
            this task too.
          </p>
          <div v-if="tryError" class="rule-dialog__error" role="alert">{{ tryError }}</div>
          <div v-else-if="tryResult" class="rule-dialog__try-result" data-testid="automation-try-result">
            <p v-if="!tryResult.matched"><strong>No match:</strong> the conditions are not true for this task.</p>
            <template v-else>
              <p><strong>It would:</strong></p>
              <ul>
                <li v-for="(item, index) in tryResult.actions" :key="index">{{ item.description }}</li>
              </ul>
              <p v-if="tryResult.task_after" class="muted">
                Afterwards: {{ tryResult.task_after.status }}
                <template v-if="tryResult.task_after.assignee"> · {{ tryResult.task_after.assignee }}</template>
                <template v-if="tryResult.task_after.tags?.length">
                  · {{ tryResult.task_after.tags.join(', ') }}</template
                >
              </p>
              <ul v-if="tryResult.warnings?.length" class="rule-dialog__warnings">
                <li v-for="(warning, index) in tryResult.warnings" :key="index">{{ warning }}</li>
              </ul>
            </template>
          </div>
        </section>

        <details class="rule-dialog__more">
          <summary>More options and YAML</summary>
          <label class="rule-dialog__field">
            <span class="rule-dialog__label">Cooldown</span>
            <input v-model="draft.cooldown" class="input" placeholder="e.g. 5m" aria-label="Cooldown" />
            <span class="rule-dialog__hint">Skip the same task if this rule ran on it this recently.</span>
          </label>
          <pre class="rule-dialog__yaml" data-testid="automation-rule-yaml">{{ previewYaml }}</pre>
        </details>
      </div>

      <footer class="rule-dialog__footer">
        <p class="rule-dialog__sentence" data-testid="automation-rule-sentence">{{ sentence }}</p>
        <UiButton type="button" @click="emit('close')">Cancel</UiButton>
        <UiButton variant="primary" type="submit" :disabled="!canSave" data-testid="automation-rule-save">
          {{ isNew ? 'Add rule' : 'Update rule' }}
        </UiButton>
      </footer>

      <datalist id="automation-status-values">
        <option v-for="value in statuses" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-priority-values">
        <option v-for="value in priorities" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-type-values">
        <option v-for="value in types" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-tag-values">
        <option v-for="value in tags" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-person-conditions">
        <option value="@agent">any agent profile</option>
        <option value="@user">any person (not an agent)</option>
        <option value="@any">anyone</option>
        <option v-for="value in people" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-person-values">
        <option value="@assignee">the current assignee</option>
        <option value="@reporter">the reporter</option>
        <option value="@assignee_or_reporter">assignee, else reporter</option>
        <option value="@me">whoever triggered it</option>
        <option value="@least_busy">the member with the least open work</option>
        <option value="@round_robin">members in turn</option>
        <option value="@random">a random member</option>
        <option v-for="value in people" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-custom-fields">
        <option v-for="value in customFields" :key="value" :value="value" />
      </datalist>
      <datalist id="automation-agent-efforts">
        <option value="low" />
        <option value="medium" />
        <option value="high" />
      </datalist>
      <datalist id="automation-preview-tasks">
        <option v-for="task in previewSuggestions" :key="task.id" :value="task.id">{{ task.title }}</option>
      </datalist>
    </form>
  </UiModal>
</template>

<script setup lang="ts">
import { computed, ref, useId, watch } from 'vue'
import { stringify as stringifyYaml } from 'yaml'
import { api } from '../../api/client'
import type { AutomationSimulateResponse, TaskDTO } from '../../api/types'
import {
  automationEventOptions,
  buildAutomationRuleFromDraft,
  createChangeDraft,
  createEmptyRuleDraft,
  createListFieldDraft,
  createSetFieldDraft,
  getEditableRuleState,
  type AutomationConditionDraft,
  type AutomationEventActionDraft,
  type AutomationEventKey,
  type AutomationRecord,
  type AutomationRuleDraft,
  type AutomationSetFieldDraft,
} from '../../utils/automationRules'
import { describeAutomationRule, templateVariables, triggerText } from '../../utils/automationText'
import IconGlyph from '../IconGlyph.vue'
import UiButton from '../UiButton.vue'
import UiModal from '../UiModal.vue'
import AutomationTemplateField from './AutomationTemplateField.vue'

/**
 * Create or edit one automation rule on a single screen (DEV-241): When
 * (triggers) / If (conditions) / Then (actions per trigger), with value
 * pickers from the project config, placeholder help, starter recipes and a
 * dry run on a real task.
 */
const props = withDefaults(
  defineProps<{
    open: boolean
    /** The rule to edit, as in automation.yml; `null` creates a new one. */
    rule: AutomationRecord | null
    statuses?: string[]
    priorities?: string[]
    types?: string[]
    tags?: string[]
    people?: string[]
    customFields?: string[]
    project?: string
  }>(),
  {
    statuses: () => [],
    priorities: () => [],
    types: () => [],
    tags: () => [],
    people: () => [],
    customFields: () => [],
    project: '',
  },
)

const emit = defineEmits<{
  (e: 'close'): void
  (e: 'save', rule: AutomationRecord): void
}>()

type Extra = 'comment' | 'run' | 'agent' | 'archive'

const draft = ref<AutomationRuleDraft>(blankDraft())
const extras = ref<Record<string, Set<Extra>>>({})
const recipeId = ref('')
const previewQuery = ref('')
const previewSuggestions = ref<Array<{ id: string; title: string }>>([])
const previewTask = ref<TaskDTO | null>(null)
const tryEvent = ref<AutomationEventKey | ''>('')
const trying = ref(false)
const tryError = ref('')
const tryResult = ref<AutomationSimulateResponse | null>(null)

const titleId = `automation-rule-title-${useId()}`
const isNew = computed(() => props.rule === null)
const title = computed(() => (isNew.value ? 'New automation rule' : 'Edit automation rule'))

// Triggers, in the order people think about them; the legacy catch-all
// `start` only shows when a rule already uses it.
const TRIGGER_HELP: Partial<Record<AutomationEventKey, string>> = {
  manual: 'Only when started: Run now, `lotar automation run`, or cron',
}
const triggerOptions = computed(() =>
  automationEventOptions
    .filter((option) => option.value !== 'start' || isTriggerOn('start'))
    .map((option) => ({
      event: option.value,
      label: option.value === 'manual' ? 'Manually / cron' : option.label,
      help: TRIGGER_HELP[option.value] ?? `When ${triggerText(option.value)}`,
    })),
)

function blankDraft(): AutomationRuleDraft {
  const next = createEmptyRuleDraft()
  next.conditions = []
  return next
}

function eventLabel(event: AutomationEventKey) {
  return triggerOptions.value.find((option) => option.event === event)?.label ?? event
}

const activeActions = computed(() => draft.value.eventActions.filter((entry) => entry.enabled))
const activeEvents = computed(() => activeActions.value.map((entry) => entry.event))
const CHANGE_TRIGGERS = new Set<AutomationEventKey>(['updated', 'assigned', 'sprint_changed', 'start'])
const hasChangeTrigger = computed(() => activeEvents.value.some((event) => CHANGE_TRIGGERS.has(event)))
const variables = computed(() => templateVariables(activeEvents.value, props.customFields))

function isTriggerOn(event: AutomationEventKey) {
  return Boolean(draft.value.eventActions.find((entry) => entry.event === event)?.enabled)
}

function toggleTrigger(event: AutomationEventKey) {
  const action = draft.value.eventActions.find((entry) => entry.event === event)
  if (!action) return
  action.enabled = !action.enabled
  if (action.enabled && !tryEvent.value) tryEvent.value = event
  if (!action.enabled && tryEvent.value === event) tryEvent.value = activeEvents.value[0] ?? ''
}

// ── conditions ──────────────────────────────────────────────────────────────

type FieldKind = 'enum' | 'person' | 'tags' | 'text' | 'date' | 'moment' | 'custom'

const conditionFields: Array<{ value: string; label: string }> = [
  { value: 'status', label: 'Status' },
  { value: 'priority', label: 'Priority' },
  { value: 'type', label: 'Type' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'reporter', label: 'Reporter' },
  { value: 'tags', label: 'Tags' },
  { value: 'title', label: 'Title' },
  { value: 'description', label: 'Description' },
  { value: 'due_date', label: 'Due date' },
  { value: 'effort', label: 'Effort' },
  { value: 'created', label: 'Created' },
  { value: 'modified', label: 'Last changed' },
  { value: 'status_changed', label: 'Time in current status' },
  { value: 'custom_field', label: 'Custom field…' },
  { value: 'other', label: 'Other field…' },
]
const KNOWN_FIELDS = new Set(conditionFields.map((option) => option.value))

function kindOf(field: string, scope: 'field' | 'custom_field' = 'field'): FieldKind {
  if (scope === 'custom_field') return 'custom'
  if (field === 'status' || field === 'priority' || field === 'type') return 'enum'
  if (field === 'assignee' || field === 'reporter') return 'person'
  if (field === 'tags' || field === 'labels') return 'tags'
  if (field === 'due_date') return 'date'
  if (field === 'created' || field === 'modified' || field === 'status_changed') return 'moment'
  return 'text'
}

function conditionFieldValue(row: AutomationConditionDraft) {
  if (row.scope === 'custom_field') return 'custom_field'
  return KNOWN_FIELDS.has(row.field) ? row.field : 'other'
}

function setConditionField(row: AutomationConditionDraft, value: string) {
  if (value === 'custom_field') {
    row.scope = 'custom_field'
    row.field = 'custom_field'
  } else if (value === 'other') {
    row.scope = 'field'
    row.field = row.customFieldKey || ''
  } else {
    row.scope = 'field'
    row.field = value
    row.customFieldKey = ''
  }
  const allowed = operatorsFor(row).map((option) => option.value)
  if (!allowed.includes(operatorValue(row))) setOperator(row, allowed[0] ?? 'equals')
}

const OPERATORS: Record<FieldKind, Array<{ value: string; label: string }>> = {
  enum: [
    { value: 'equals', label: 'is' },
    { value: 'in', label: 'is one of' },
    { value: 'exists:false', label: 'is empty' },
  ],
  person: [
    { value: 'equals', label: 'is' },
    { value: 'in', label: 'is one of' },
    { value: 'exists:true', label: 'is set' },
    { value: 'exists:false', label: 'is empty' },
  ],
  tags: [
    { value: 'any', label: 'include any of' },
    { value: 'all', label: 'include all of' },
    { value: 'none', label: 'include none of' },
    { value: 'exists:false', label: 'are empty' },
  ],
  text: [
    { value: 'contains', label: 'contains' },
    { value: 'equals', label: 'is' },
    { value: 'starts_with', label: 'starts with' },
    { value: 'matches', label: 'matches pattern' },
    { value: 'exists:true', label: 'is set' },
    { value: 'exists:false', label: 'is empty' },
  ],
  date: [
    { value: 'within', label: 'is within the next' },
    { value: 'before', label: 'is before' },
    { value: 'older_than', label: 'passed more than' },
    { value: 'exists:false', label: 'is not set' },
  ],
  moment: [
    { value: 'older_than', label: 'more than … ago' },
    { value: 'within', label: 'in the last' },
  ],
  custom: [
    { value: 'equals', label: 'is' },
    { value: 'contains', label: 'contains' },
    { value: 'in', label: 'is one of' },
    { value: 'exists:true', label: 'is set' },
    { value: 'exists:false', label: 'is empty' },
  ],
}

function operatorsFor(row: AutomationConditionDraft) {
  const options = OPERATORS[kindOf(row.field, row.scope)]
  const current = operatorValue(row)
  // Keep an operator written by hand visible even if the menu would not offer it.
  return options.some((option) => option.value === current) ? options : [...options, { value: current, label: current }]
}

function operatorValue(row: AutomationConditionDraft) {
  return row.operator === 'exists' ? `exists:${row.value === 'false' ? 'false' : 'true'}` : row.operator
}

function setOperator(row: AutomationConditionDraft, value: string) {
  if (value.startsWith('exists:')) {
    row.operator = 'exists'
    row.value = value.slice('exists:'.length)
    return
  }
  if (row.operator === 'exists') row.value = ''
  row.operator = value as AutomationConditionDraft['operator']
}

function valuePlaceholder(row: AutomationConditionDraft) {
  const kind = kindOf(row.field, row.scope)
  if (kind === 'moment') return 'e.g. 30d, 12h, 2w'
  if (kind === 'date') return row.operator === 'before' ? 'today, or 2026-12-01' : 'e.g. 7d'
  if (row.operator === 'in' || kind === 'tags') return 'comma separated'
  if (row.operator === 'matches') return 'regular expression'
  return 'value'
}

function valueListFor(field: string, use: 'condition' | 'set') {
  if (field === 'status') return 'automation-status-values'
  if (field === 'priority') return 'automation-priority-values'
  if (field === 'type') return 'automation-type-values'
  if (field === 'tags' || field === 'labels') return 'automation-tag-values'
  if (field === 'assignee' || field === 'reporter') {
    return use === 'condition' ? 'automation-person-conditions' : 'automation-person-values'
  }
  return undefined
}

function addCondition() {
  const row = createEmptyRuleDraft().conditions[0]!
  row.field = 'status'
  row.operator = 'equals'
  draft.value.conditions.push(row)
}

function removeCondition(id: string) {
  draft.value.conditions = draft.value.conditions.filter((row) => row.id !== id)
}

const changeFields = [
  { value: 'status', label: 'Status' },
  { value: 'priority', label: 'Priority' },
  { value: 'type', label: 'Type' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'reporter', label: 'Reporter' },
]

function addChange() {
  const row = createChangeDraft()
  row.field = 'status'
  draft.value.changeConditions.push(row)
}

function removeChange(id: string) {
  draft.value.changeConditions = draft.value.changeConditions.filter((row) => row.id !== id)
}

// ── actions ─────────────────────────────────────────────────────────────────

const setFields = [
  { value: 'status', label: 'Status' },
  { value: 'priority', label: 'Priority' },
  { value: 'type', label: 'Type' },
  { value: 'assignee', label: 'Assignee' },
  { value: 'reporter', label: 'Reporter' },
  { value: 'due_date', label: 'Due date' },
  { value: 'effort', label: 'Effort' },
  { value: 'title', label: 'Title' },
  { value: 'description', label: 'Description' },
  { value: 'tags', label: 'All tags (replace)' },
  { value: 'custom_field', label: 'Custom field…' },
]

const listFields = [
  { value: 'tags', label: 'tags' },
  { value: 'sprint', label: 'sprint' },
  { value: 'depends_on', label: '“depends on” link' },
  { value: 'blocks', label: '“blocks” link' },
  { value: 'related', label: '“related” link' },
] as const

function isTextField(row: AutomationSetFieldDraft) {
  return row.scope === 'custom_field' || row.field === 'title' || row.field === 'description'
}

function setPlaceholder(field: string) {
  if (field === 'due_date') return 'e.g. 2026-12-01, +3d, next friday'
  if (field === 'effort') return 'e.g. 3d, 5h, 2pt'
  if (field === 'tags') return 'comma separated'
  return 'value'
}

function listPlaceholder(field: string) {
  if (field === 'sprint') return '@active, or a sprint number'
  if (field === 'tags' || field === 'labels') return 'comma separated'
  return 'task IDs, comma separated'
}

function setSetField(row: AutomationSetFieldDraft, value: string) {
  if (value === 'custom_field') {
    row.scope = 'custom_field'
    row.field = 'custom_field'
  } else {
    row.scope = 'field'
    row.field = value
    row.customFieldKey = ''
  }
}

function extrasOf(action: AutomationEventActionDraft): Set<Extra> {
  const key = action.event
  if (!extras.value[key]) extras.value[key] = new Set()
  return extras.value[key]!
}

function shows(action: AutomationEventActionDraft, extra: Extra) {
  return extrasOf(action).has(extra)
}

function clearExtra(action: AutomationEventActionDraft, extra: Extra) {
  extrasOf(action).delete(extra)
  if (extra === 'comment') action.comment = ''
  if (extra === 'run') {
    action.runMode = 'none'
    action.runShell = ''
    action.runCommand = ''
    action.runArgs = ''
    action.runCwd = ''
  }
  if (extra === 'agent') {
    action.agentModel = ''
    action.agentEffort = ''
    action.agentPrompt = ''
  }
  if (extra === 'archive') action.archive = ''
}

function actionMenu(action: AutomationEventActionDraft) {
  const has = (extra: Extra) => shows(action, extra)
  return [
    { value: 'set:status', label: 'Set the status', disabled: false },
    { value: 'set:assignee', label: 'Assign to someone or an agent', disabled: false },
    { value: 'set:priority', label: 'Set the priority', disabled: false },
    { value: 'set:other', label: 'Set another field', disabled: false },
    { value: 'add:tags', label: 'Add tags', disabled: false },
    { value: 'remove:tags', label: 'Remove tags', disabled: false },
    { value: 'add:sprint', label: 'Add to a sprint', disabled: false },
    { value: 'remove:sprint', label: 'Remove from a sprint', disabled: false },
    { value: 'add:depends_on', label: 'Link another task', disabled: false },
    { value: 'comment', label: 'Add a comment', disabled: has('comment') },
    { value: 'run', label: 'Run a command', disabled: has('run') },
    { value: 'agent', label: 'Agent run settings (model, prompt)', disabled: has('agent') },
    { value: 'archive', label: 'Archive the task', disabled: has('archive') },
  ]
}

function addAction(action: AutomationEventActionDraft, choice: string) {
  if (!choice) return
  if (choice.startsWith('set:')) {
    const row = createSetFieldDraft()
    const field = choice.slice(4)
    row.field = field === 'other' ? 'due_date' : field
    action.setFields.push(row)
    return
  }
  if (choice.startsWith('add:') || choice.startsWith('remove:')) {
    const [kind, field] = choice.split(':') as ['add' | 'remove', string]
    const row = createListFieldDraft(field as Parameters<typeof createListFieldDraft>[0])
    ;(kind === 'add' ? action.addFields : action.removeFields).push(row)
    return
  }
  const extra = choice as Extra
  extrasOf(action).add(extra)
  if (extra === 'run' && action.runMode === 'none') action.runMode = 'shell'
  if (extra === 'archive' && !action.archive) action.archive = 'archive'
}

function onAddActionSelect(action: AutomationEventActionDraft, event: Event) {
  const select = event.target as HTMLSelectElement
  addAction(action, select.value)
  select.value = ''
}

function removeSet(action: AutomationEventActionDraft, id: string) {
  action.setFields = action.setFields.filter((row) => row.id !== id)
}

function removeList(action: AutomationEventActionDraft, kind: 'add' | 'remove', id: string) {
  if (kind === 'add') action.addFields = action.addFields.filter((row) => row.id !== id)
  else action.removeFields = action.removeFields.filter((row) => row.id !== id)
}

function actionHasContent(action: AutomationEventActionDraft) {
  return Boolean(
    action.setFields.some((row) => row.value.trim()) ||
    action.addFields.some((row) => row.value.trim()) ||
    action.removeFields.some((row) => row.value.trim()) ||
    action.comment.trim() ||
    (action.runMode === 'shell' && action.runShell.trim()) ||
    (action.runMode === 'command' && action.runCommand.trim()) ||
    action.agentModel.trim() ||
    action.agentEffort.trim() ||
    action.agentPrompt.trim() ||
    action.archive,
  )
}

// ── result ──────────────────────────────────────────────────────────────────

const builtRule = computed(() => buildAutomationRuleFromDraft(draft.value))
const previewYaml = computed(() => stringifyYaml({ automation: { rules: [builtRule.value] } }))
const canSave = computed(() => activeActions.value.length > 0 && activeActions.value.every(actionHasContent))

const sentence = computed(() => {
  if (!activeActions.value.length) return 'Pick a trigger and add an action.'
  const described = describeAutomationRule(builtRule.value)
  const conditions = described.conditions.items.length
    ? `, if ${described.conditions.items.join(described.conditions.join === 'any' ? ' or ' : ' and ')}`
    : ''
  const lines = described.triggers.map(
    (trigger) => `When ${trigger.when}${conditions}: ${trigger.steps.join(', ') || 'nothing yet'}.`,
  )
  return `${draft.value.enabled ? '' : '(Off) '}${lines.join(' ')}`
})

function save() {
  if (!canSave.value) return
  emit('save', builtRule.value)
}

// ── recipes ─────────────────────────────────────────────────────────────────

interface Recipe {
  id: string
  label: string
  description: string
  rule: () => AutomationRecord
}

function pickStatus(...needles: string[]): string {
  for (const needle of needles) {
    const found = props.statuses.find((status) =>
      status
        .toLowerCase()
        .replace(/[^a-z]/g, '')
        .includes(needle),
    )
    if (found) return found
  }
  return needles[0] ?? ''
}

const recipes = computed<Recipe[]>(() => [
  {
    id: 'blank',
    label: 'Blank',
    description: 'Start empty',
    rule: () => ({ on: {} }),
  },
  {
    id: 'agent-handback',
    label: 'Hand back after an agent',
    description: 'When an agent finishes, move the task to review and give it back to the reporter',
    rule: () => ({
      name: 'Hand back after an agent',
      when: { assignee: '@agent' },
      on: {
        job_started: { set: { status: pickStatus('inprogress', 'doing') } },
        job_completed: {
          set: { status: pickStatus('review', 'done'), assignee: '@reporter' },
          comment: 'Agent finished on ${{agent.worktree_branch}}. Ready for review.',
        },
      },
    }),
  },
  {
    id: 'archive-done',
    label: 'Archive done work after 30 days',
    description: 'A manual rule for cron: archive tasks that have been done for a month',
    rule: () => ({
      name: 'Archive finished work',
      when: { all: [{ status: pickStatus('done') }, { status_changed: { older_than: '30d' } }] },
      on: { manual: { archive: true } },
    }),
  },
  {
    id: 'tag-urgent',
    label: 'Tag urgent new tasks',
    description: 'When a task is created with the highest priority, tag it',
    rule: () => ({
      name: 'Tag urgent work',
      when: { priority: props.priorities[props.priorities.length - 1] ?? 'High' },
      on: { created: { add: { tags: ['urgent'] } } },
    }),
  },
  {
    id: 'review-comment',
    label: 'Ask for review',
    description: 'Comment when a task moves into review',
    rule: () => ({
      name: 'Ask for review',
      when: { changes: { status: { to: pickStatus('review') } } },
      on: { updated: { comment: '@${{ticket.reporter}} ${{ticket.id}} is ready for review.' } },
    }),
  },
])

function applyRecipe(recipe: Recipe) {
  recipeId.value = recipe.id
  load(recipe.rule())
}

// ── loading and trying ─────────────────────────────────────────────────────

function load(rule: AutomationRecord | null) {
  const next = rule ? getEditableRuleState(rule).draft : blankDraft()
  next.conditions = next.conditions.filter((row) => row.field.trim() || row.customFieldKey.trim() || row.value.trim())
  draft.value = next
  const nextExtras: Record<string, Set<Extra>> = {}
  for (const action of next.eventActions) {
    const set = new Set<Extra>()
    if (action.comment.trim()) set.add('comment')
    if (action.runMode !== 'none') set.add('run')
    if (action.agentModel || action.agentEffort || action.agentPrompt) set.add('agent')
    if (action.archive) set.add('archive')
    nextExtras[action.event] = set
  }
  extras.value = nextExtras
  tryEvent.value = next.eventActions.find((entry) => entry.enabled)?.event ?? ''
  tryResult.value = null
  tryError.value = ''
}

watch(
  () => [props.open, props.rule] as const,
  ([isOpen]) => {
    if (!isOpen) return
    recipeId.value = ''
    load(props.rule)
  },
  { immediate: true },
)

let searchTimer: ReturnType<typeof setTimeout> | null = null
function searchPreviewTasks() {
  if (searchTimer) clearTimeout(searchTimer)
  const query = previewQuery.value.trim()
  if (!query) return
  searchTimer = setTimeout(async () => {
    try {
      previewSuggestions.value = await api.suggestTasks(query, props.project || undefined, 8)
    } catch {
      previewSuggestions.value = []
    }
  }, 200)
}

async function loadPreviewTask() {
  const id = previewQuery.value.trim()
  if (!id) {
    previewTask.value = null
    return
  }
  try {
    previewTask.value = await api.getTask(id)
  } catch {
    previewTask.value = null
  }
}

async function tryRule() {
  if (!tryEvent.value || !previewQuery.value.trim()) return
  trying.value = true
  tryError.value = ''
  tryResult.value = null
  try {
    await loadPreviewTask()
    tryResult.value = await api.simulateAutomation({
      ticket_id: previewQuery.value.trim(),
      event: tryEvent.value,
      rule: builtRule.value,
    })
  } catch (error) {
    tryError.value = (error as Error | undefined)?.message || 'Could not try the rule'
  } finally {
    trying.value = false
  }
}
</script>

<style scoped>
.rule-dialog {
  display: flex;
  flex-direction: column;
  max-height: min(90vh, 900px);
}

.rule-dialog__header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: var(--space-4, 1rem) var(--space-5, 1.5rem) var(--space-2, 0.5rem);
}

.rule-dialog__header h2 {
  margin: 0;
  font-size: var(--text-lg, 1.25rem);
}

.rule-dialog__body {
  overflow-y: auto;
  padding: 0 var(--space-5, 1.5rem) var(--space-4, 1rem);
  display: flex;
  flex-direction: column;
  gap: var(--space-4, 1rem);
}

.rule-dialog__recipes {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  flex-wrap: wrap;
}

.rule-dialog__label {
  font-size: var(--text-xs, 0.75rem);
  font-weight: 600;
  color: var(--color-muted);
}

.rule-dialog__name-row {
  display: flex;
  gap: var(--space-3, 0.75rem);
  align-items: flex-end;
}

.rule-dialog__field {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.rule-dialog__name {
  flex: 1;
}

.rule-dialog__switch {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  padding-bottom: 8px;
  font-weight: 500;
}

.rule-dialog__section {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
  padding-top: var(--space-3, 0.75rem);
  border-top: 1px solid var(--color-border);
}

.rule-dialog__section-title {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  margin: 0;
  font-size: var(--text-base, 1rem);
  font-weight: 500;
}

.rule-dialog__keyword {
  font-weight: 700;
  min-width: 3.5rem;
}

.rule-dialog__chips {
  display: flex;
  flex-wrap: wrap;
  gap: 6px;
}

.rule-dialog__chip {
  border: 1px solid var(--color-border);
  border-radius: var(--radius-pill, 999px);
  background: var(--color-surface);
  color: var(--color-fg);
  padding: 4px 12px;
  font-size: var(--text-sm, 0.875rem);
  cursor: pointer;
}

.rule-dialog__chip.is-active {
  border-color: var(--color-accent);
  background: color-mix(in oklab, var(--color-accent) 16%, var(--color-surface));
  font-weight: 600;
}

.rule-dialog__row {
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.rule-dialog__row--top {
  align-items: flex-start;
}

.rule-dialog__word {
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
  min-width: 3.5rem;
}

.rule-dialog__field-select {
  width: auto;
  min-width: 10rem;
}

.rule-dialog__op-select {
  width: auto;
  min-width: 9rem;
}

.rule-dialog__key-input {
  width: 9rem;
}

.rule-dialog__value-input {
  flex: 1;
  min-width: 10rem;
}

.rule-dialog__inline-select {
  width: auto;
}

.rule-dialog__remove {
  border: 0;
  background: transparent;
  color: var(--color-muted);
  cursor: pointer;
  padding: 4px;
  border-radius: var(--radius-sm);
}

.rule-dialog__remove:hover {
  color: var(--color-danger);
  background: color-mix(in oklab, var(--color-danger) 10%, transparent);
}

.rule-dialog__add-row {
  display: flex;
  align-items: center;
  gap: var(--space-3, 0.75rem);
  flex-wrap: wrap;
}

.rule-dialog__add {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  border: 1px dashed var(--color-border);
  border-radius: var(--radius-md);
  background: transparent;
  color: var(--color-muted);
  padding: 4px 10px;
  cursor: pointer;
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__add:hover {
  color: var(--color-fg);
  border-color: var(--color-accent);
}

.rule-dialog__add-select {
  width: auto;
  border-style: dashed;
  color: var(--color-muted);
}

.rule-dialog__event-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding: var(--space-2, 0.5rem) 0;
}

.rule-dialog__event-title {
  margin: 0;
  font-weight: 600;
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__stack {
  display: flex;
  flex-direction: column;
  gap: 6px;
  flex: 1;
  min-width: 0;
}

.rule-dialog__inline {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}

.rule-dialog__check {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__hint {
  margin: 0;
  font-size: var(--text-xs, 0.75rem);
  color: var(--color-muted);
}

.rule-dialog__hint code {
  font-size: 11px;
}

.rule-dialog__try-result {
  padding: var(--space-2, 0.5rem) var(--space-3, 0.75rem);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__try-result p,
.rule-dialog__try-result ul {
  margin: 4px 0;
}

.rule-dialog__warnings {
  color: var(--color-warning, var(--color-muted));
}

.rule-dialog__error {
  color: var(--color-danger);
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__more summary {
  cursor: pointer;
  color: var(--color-muted);
  font-size: var(--text-sm, 0.875rem);
}

.rule-dialog__more[open] {
  display: flex;
  flex-direction: column;
  gap: var(--space-2, 0.5rem);
}

.rule-dialog__yaml {
  margin: 0;
  padding: var(--space-2, 0.5rem);
  border-radius: var(--radius-md);
  background: var(--color-surface);
  font-size: 12px;
  overflow-x: auto;
}

.rule-dialog__footer {
  display: flex;
  align-items: center;
  gap: var(--space-2, 0.5rem);
  padding: var(--space-3, 0.75rem) var(--space-5, 1.5rem);
  border-top: 1px solid var(--color-border);
}

.rule-dialog__sentence {
  flex: 1;
  margin: 0;
  font-size: var(--text-sm, 0.875rem);
  color: var(--color-muted);
}

@media (max-width: 640px) {
  .rule-dialog__field-select,
  .rule-dialog__op-select,
  .rule-dialog__value-input,
  .rule-dialog__key-input {
    width: 100%;
    min-width: 0;
    flex: 1 1 100%;
  }

  .rule-dialog__footer {
    flex-wrap: wrap;
  }
}
</style>
