<template>
  <div class="automation-builder">
    <div class="automation-builder__toolbar">
      <div class="col" style="gap: 4px;">
        <h3 class="automation-builder__title">Rule builder</h3>
        <p class="muted automation-builder__subtitle">
          Start with the outcome you want, then choose the trigger and any optional filters.
        </p>
      </div>
      <UiButton
        variant="primary"
        type="button"
        :disabled="loading || hasParseError || !editable"
        @click="openCreateDialog"
      >
        <IconGlyph name="plus" />
        <span>New rule</span>
      </UiButton>
    </div>

    <div class="automation-builder__meta row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
      <span v-if="sourceLabel" class="muted">Effective source: {{ sourceLabel }}</span>
      <span v-if="scopeHint" class="muted">{{ scopeHint }}</span>
    </div>

    <p v-if="error" class="field-error">{{ error }}</p>
    <p v-if="parsedDocument.parseError" class="field-error">
      YAML must be valid before the visual builder can edit it: {{ parsedDocument.parseError }}
    </p>

    <UiEmptyState
      v-if="!builderRules.length && !hasParseError"
      title="No automation rules yet"
      description="Create a rule with the guided builder, or open the advanced YAML editor below."
    />

    <div v-else class="automation-builder__rule-list">
      <UiCard v-for="(entry, index) in builderRules" :key="entry.id" class="automation-builder__rule-card rule-card">
        <div class="automation-builder__rule-head">
          <div class="col" style="gap: 10px; flex: 1 1 auto;">
            <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
              <strong class="rule-name">{{ formatAutomationRuleLabel(entry.summary, index) }}</strong>
              <span v-for="event in entry.summary.events" :key="event" class="automation-builder__event-chip">
                {{ event }}
              </span>
              <span v-if="entry.rule.cooldown" class="automation-builder__meta-chip">Cooldown {{ entry.rule.cooldown }}</span>
              <span v-if="entry.unsupportedReasons.length" class="automation-builder__warning-chip">YAML-only</span>
            </div>

            <p class="automation-builder__rule-sentence">
              {{ describeRuleCard(entry.summary) }}
            </p>

            <div v-if="entry.summary.conditions.length" class="automation-builder__summary-block">
              <span class="muted">Filters</span>
              <div class="automation-builder__summary-list">
                <span v-for="condition in entry.summary.conditions.slice(0, 3)" :key="condition" class="automation-builder__summary-chip">
                  {{ condition }}
                </span>
                <span v-if="entry.summary.conditions.length > 3" class="automation-builder__summary-chip">
                  +{{ entry.summary.conditions.length - 3 }} more
                </span>
              </div>
            </div>

            <div class="automation-builder__summary-block">
              <span class="muted">Results</span>
              <div class="automation-builder__summary-list">
                <span v-for="action in entry.summary.actions.slice(0, 3)" :key="action" class="automation-builder__summary-chip automation-builder__summary-chip--action">
                  {{ action }}
                </span>
                <span v-if="entry.summary.actions.length > 3" class="automation-builder__summary-chip automation-builder__summary-chip--action">
                  +{{ entry.summary.actions.length - 3 }} more
                </span>
              </div>
            </div>

            <div v-if="entry.unsupportedReasons.length" class="automation-builder__rule-note">
              This rule uses advanced YAML features that the guided editor does not currently model.
            </div>
          </div>

          <div class="automation-builder__rule-actions">
            <UiButton
              variant="ghost"
              type="button"
              :disabled="!editable || !!entry.unsupportedReasons.length"
              @click="openEditDialog(index)"
            >
              <IconGlyph name="edit" />
              <span>Edit</span>
            </UiButton>
            <UiButton variant="ghost" type="button" :disabled="!editable" @click="duplicateRule(index)">
              Duplicate
            </UiButton>
            <UiButton variant="ghost" type="button" :disabled="!editable || index === 0" @click="moveRule(index, -1)">
              Up
            </UiButton>
            <UiButton variant="ghost" type="button" :disabled="!editable || index === builderRules.length - 1" @click="moveRule(index, 1)">
              Down
            </UiButton>
            <UiButton variant="danger" type="button" :disabled="!editable" @click="deleteRule(index)">
              Delete
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
          <textarea class="automation-builder__yaml automation-builder__yaml--readonly" rows="10" :value="effectiveYaml" readonly></textarea>
        </div>
      </div>
    </details>

    <div v-if="dialogOpen" class="automation-builder__dialog-backdrop" @click.self="closeDialog">
      <div class="automation-builder__dialog card" role="dialog" aria-modal="true">
        <header class="automation-builder__dialog-header">
          <div>
            <h2>{{ editingIndex === null ? 'Create automation rule' : 'Edit automation rule' }}</h2>
            <p class="muted">Focus on the common path first. Use Custom only when you really need multiple triggers or mixed actions.</p>
          </div>
          <UiButton variant="ghost" icon-only type="button" aria-label="Close dialog" title="Close dialog" @click="closeDialog">
            <IconGlyph name="close" />
          </UiButton>
        </header>

        <div class="automation-builder__stepper" aria-label="Rule builder progress">
          <div
            v-for="(step, index) in stepLabels"
            :key="step"
            class="automation-builder__step"
            :class="{
              'automation-builder__step--active': index === dialogStep,
              'automation-builder__step--complete': index < dialogStep,
            }"
          >
            <span>{{ index + 1 }}</span>
            <span>{{ step }}</span>
          </div>
        </div>

        <div class="automation-builder__dialog-body">
          <section v-if="dialogStep === 0" class="col" style="gap: 18px;">
            <div class="automation-builder__guide-block">
              <h3>What should this rule do?</h3>
              <p class="muted">Most rules have one primary job. Pick that first and the rest of the dialog will stay focused.</p>
            </div>

            <div class="automation-builder__recipe-grid">
              <button
                v-for="recipe in recipeOptions"
                :key="recipe.value"
                type="button"
                class="automation-builder__recipe-card"
                :class="{ 'automation-builder__recipe-card--active': dialogRecipe === recipe.value }"
                @click="selectRecipe(recipe.value)"
              >
                <strong>{{ recipe.label }}</strong>
                <p>{{ recipe.description }}</p>
                <span class="muted">{{ recipe.example }}</span>
              </button>
            </div>

            <div v-if="dialogRecipe" class="field-grid">
              <div class="field">
                <label class="field-label">Rule name</label>
                <UiInput v-model="dialogDraft.name" maxlength="120" placeholder="Human review after testing" />
                <p class="field-hint">Optional, but helpful when you have several rules.</p>
              </div>
              <div class="field">
                <label class="field-label">Cooldown</label>
                <UiInput v-model="dialogDraft.cooldown" maxlength="32" placeholder="60s, 5m, 1h" />
                <p class="field-hint">Optional. Prevents the same rule from firing repeatedly in a short window.</p>
              </div>
            </div>
          </section>

          <section v-else-if="dialogStep === 1" class="col" style="gap: 18px;">
            <div class="automation-builder__guide-block">
              <h3>When should it run?</h3>
              <p class="muted">Choose the trigger first, then add filters only if the rule should apply to a subset of tasks.</p>
            </div>

            <div v-if="dialogRecipe === 'custom'" class="field">
              <label class="field-label">Triggers</label>
              <p class="field-hint">Custom rules can react to multiple events. Common rules usually only need one.</p>
              <div class="automation-builder__event-grid">
                <button
                  v-for="option in automationEventOptions"
                  :key="option.value"
                  type="button"
                  class="automation-builder__event-toggle"
                  :class="{ 'automation-builder__event-toggle--active': isCustomEventEnabled(option.value) }"
                  @click="toggleCustomEvent(option.value)"
                >
                  <strong>{{ option.label }}</strong>
                  <span>{{ eventDetails[option.value] }}</span>
                </button>
              </div>
            </div>

            <div v-else class="field">
              <label class="field-label">Trigger</label>
              <p class="field-hint">Pick the single moment when this rule should run.</p>
              <div class="automation-builder__event-grid">
                <button
                  v-for="option in automationEventOptions"
                  :key="option.value"
                  type="button"
                  class="automation-builder__event-toggle automation-builder__event-toggle--single"
                  :class="{ 'automation-builder__event-toggle--active': selectedSimpleEvent === option.value }"
                  @click="selectedSimpleEvent = option.value"
                >
                  <strong>{{ option.label }}</strong>
                  <span>{{ eventDetails[option.value] }}</span>
                </button>
              </div>
            </div>

            <div class="field">
              <label class="field-label">Optional filters</label>
              <p class="field-hint">Leave these empty if the rule should always run when the trigger fires.</p>

              <div v-if="!visibleConditions.length && !visibleChangeConditions.length" class="automation-builder__inline-empty">
                No extra filters yet.
              </div>

              <div v-if="visibleConditions.length" class="automation-builder__rows">
                <div v-for="row in visibleConditions" :key="row.id" class="automation-builder__row automation-builder__row--condition">
                  <UiSelect :model-value="getConditionFieldSelection(row)" @update:modelValue="(value) => handleConditionFieldChange(row, value as any)">
                    <option v-for="option in automationConditionFieldOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
                  </UiSelect>
                  <UiInput
                    v-if="row.scope === 'custom_field'"
                    v-model="row.customFieldKey"
                    maxlength="80"
                    placeholder="Custom field key"
                  />
                  <UiInput
                    v-else-if="getConditionFieldSelection(row) === 'other'"
                    v-model="row.field"
                    maxlength="80"
                    placeholder="Field name"
                  />
                  <UiSelect v-model="row.operator">
                    <option v-for="option in automationConditionOperatorOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
                  </UiSelect>
                  <UiSelect v-if="row.operator === 'exists'" v-model="row.value">
                    <option value="true">Exists</option>
                    <option value="false">Does not exist</option>
                  </UiSelect>
                  <UiInput
                    v-else
                    v-model="row.value"
                    maxlength="240"
                    :placeholder="conditionValuePlaceholder(row.operator)"
                    :list="conditionValueSuggestions(row).length ? `cond-suggestions-${row.id}` : undefined"
                  />
                  <datalist v-if="conditionValueSuggestions(row).length" :id="`cond-suggestions-${row.id}`">
                    <option v-for="s in conditionValueSuggestions(row)" :key="s" :value="s" />
                  </datalist>
                  <UiButton variant="danger" type="button" @click="removeCondition(row.id)">Remove</UiButton>
                </div>
              </div>

              <div v-if="canUseChangeConditions && visibleChangeConditions.length" class="automation-builder__rows" style="margin-top: 10px;">
                <div v-for="row in visibleChangeConditions" :key="row.id" class="automation-builder__row automation-builder__row--change">
                  <UiInput v-model="row.field" maxlength="80" placeholder="Field name" />
                  <UiInput v-model="row.from" maxlength="120" placeholder="From value" />
                  <UiInput v-model="row.to" maxlength="120" placeholder="To value" />
                  <UiButton variant="danger" type="button" @click="removeChangeCondition(row.id)">Remove</UiButton>
                </div>
              </div>

              <div class="row" style="gap: 8px; margin-top: 12px; flex-wrap: wrap;">
                <UiButton variant="ghost" type="button" @click="addCondition">Add filter</UiButton>
                <UiButton v-if="canUseChangeConditions" variant="ghost" type="button" @click="addChangeCondition">Watch a field change</UiButton>
              </div>
            </div>
          </section>

          <section v-else-if="dialogStep === 2" class="col" style="gap: 18px;">
            <div class="automation-builder__guide-block">
              <h3>What should happen?</h3>
              <p class="muted">Only the controls for the selected goal are shown here. Use Custom if you need a more complex combination.</p>
            </div>

            <UiEmptyState
              v-if="dialogRecipe !== 'custom' && !selectedSimpleEvent"
              title="Choose a trigger first"
              description="Go back one step and pick when this rule should run."
            />

            <UiEmptyState
              v-else-if="dialogRecipe === 'custom' && !customEnabledEventActions.length"
              title="Choose at least one trigger"
              description="Custom rules need one or more enabled events before you can configure the result."
            />

            <template v-else-if="dialogRecipe === 'status'">
              <div class="field-grid">
                <div class="field">
                  <label class="field-label">Set status to</label>
                  <UiInput v-model="simpleStatusValue" maxlength="120" placeholder="Done" list="automation-status-suggestions" />
                  <datalist v-if="props.availableStatuses.length" id="automation-status-suggestions">
                    <option v-for="s in props.availableStatuses" :key="s" :value="s" />
                  </datalist>
                  <p class="field-hint">{{ props.availableStatuses.length ? 'Choose from your configured statuses or type a custom value.' : 'Example: InProgress, Review, Done.' }}</p>
                </div>
              </div>
            </template>

            <template v-else-if="dialogRecipe === 'assignment'">
              <div class="field-grid">
                <div class="field">
                  <label class="field-label">Update</label>
                  <UiSelect v-model="simpleAssignmentField">
                    <option value="assignee">Assignee</option>
                    <option value="reporter">Reporter</option>
                  </UiSelect>
                </div>
                <div class="field">
                  <label class="field-label">Value</label>
                  <UiInput v-model="simpleAssignmentValue" maxlength="160" placeholder="@assignee_or_reporter" list="automation-member-suggestions" />
                  <datalist v-if="memberSuggestions.length" id="automation-member-suggestions">
                    <option v-for="m in memberSuggestions" :key="m" :value="m" />
                  </datalist>
                  <p class="field-hint">Tokens like @assignee, @reporter, @assignee_or_reporter, and @me are supported.{{ props.availableMembers.length ? ' Or choose a team member.' : '' }}</p>
                </div>
              </div>
            </template>

            <template v-else-if="dialogRecipe === 'tags'">
              <div class="field-grid">
                <div class="field">
                  <label class="field-label">Tag action</label>
                  <UiSelect v-model="simpleTagMode">
                    <option value="add">Add tags</option>
                    <option value="remove">Remove tags</option>
                  </UiSelect>
                </div>
              </div>
              <div class="field">
                <label class="field-label">Tags</label>
                <ChipListField
                  v-model="simpleTagValues"
                  :suggestions="props.availableTags"
                  empty-label="No tags selected yet."
                  add-label="Add tag"
                  composer-label="Tag"
                  composer-confirm-label="Add tag"
                  placeholder="ready-for-review"
                />
              </div>
            </template>

            <template v-else-if="dialogRecipe === 'comment'">
              <div class="field">
                <label class="field-label">Comment</label>
                <textarea v-model="simpleCommentValue" class="automation-builder__textarea" rows="5" placeholder="Ready for review"></textarea>
                <p class="field-hint">Template variables such as ${ticket.status} and ${previous.status} are supported.</p>
              </div>
            </template>

            <template v-else-if="dialogRecipe === 'command'">
              <div class="field-grid">
                <div class="field">
                  <label class="field-label">Command style</label>
                  <UiSelect v-model="simpleCommandMode">
                    <option value="shell">Shell command</option>
                    <option value="command">Structured command</option>
                  </UiSelect>
                </div>
              </div>

              <div v-if="simpleCommandMode === 'shell'" class="field">
                <label class="field-label">Shell command</label>
                <textarea v-model="simpleCommandShell" class="automation-builder__textarea" rows="4" placeholder="sh -c 'echo $LOTAR_TICKET_ID'"></textarea>
                <p class="field-hint">Use this for the common case. Structured mode is only needed when you want args, cwd, or extra environment variables.</p>
              </div>

              <div v-else class="col" style="gap: 14px;">
                <div class="field-grid">
                  <div class="field">
                    <label class="field-label">Command</label>
                    <UiInput v-model="simpleCommandSpec.command" maxlength="200" placeholder="lotar" />
                  </div>
                  <div class="field">
                    <label class="field-label">Args</label>
                    <UiInput v-model="simpleCommandSpec.args" maxlength="240" placeholder="Comma separated args" />
                  </div>
                  <div class="field">
                    <label class="field-label">Working directory</label>
                    <UiInput v-model="simpleCommandSpec.cwd" maxlength="240" placeholder="Optional cwd" />
                  </div>
                </div>

                <div class="automation-builder__checkbox-row">
                  <label><input v-model="simpleCommandSpec.wait" type="checkbox"> Wait for command</label>
                  <label><input v-model="simpleCommandSpec.ignoreFailure" type="checkbox"> Ignore failures</label>
                </div>

                <div class="field">
                  <label class="field-label">Extra environment</label>
                  <div class="automation-builder__rows">
                    <div v-for="env in simpleCommandSpec.env" :key="env.id" class="automation-builder__row automation-builder__row--env">
                      <UiInput v-model="env.key" maxlength="80" placeholder="ENV_NAME" />
                      <UiInput v-model="env.value" maxlength="240" placeholder="Value" />
                      <UiButton variant="danger" type="button" @click="removeSimpleCommandEnv(env.id)">Remove</UiButton>
                    </div>
                  </div>
                  <div class="row" style="gap: 8px; margin-top: 10px;">
                    <UiButton variant="ghost" type="button" @click="addSimpleCommandEnv">Add env var</UiButton>
                  </div>
                </div>
              </div>
            </template>

            <template v-else-if="dialogRecipe === 'custom'">
              <div class="field">
                <label class="field-label">Configure trigger result</label>
                <p class="field-hint">Choose one enabled event at a time to keep the custom path manageable.</p>
                <div class="automation-builder__event-tabs">
                  <button
                    v-for="entry in customEnabledEventActions"
                    :key="entry.event"
                    type="button"
                    class="automation-builder__event-tab"
                    :class="{ 'automation-builder__event-tab--active': activeCustomEvent === entry.event }"
                    @click="activeCustomEvent = entry.event"
                  >
                    {{ getAutomationEventLabel(entry.event) }}
                  </button>
                </div>
              </div>

              <UiCard v-if="activeCustomAction" class="automation-builder__event-card">
                <div class="col" style="gap: 14px;">
                  <div class="row" style="justify-content: space-between; align-items: center; gap: 12px; flex-wrap: wrap;">
                    <strong>{{ getAutomationEventLabel(activeCustomAction.event) }}</strong>
                    <span class="automation-builder__event-chip">{{ activeCustomAction.event }}</span>
                  </div>

                  <div class="field">
                    <label class="field-label">Set fields</label>
                    <div class="automation-builder__rows">
                      <div v-for="row in activeCustomAction.setFields" :key="row.id" class="automation-builder__row automation-builder__row--set">
                        <UiSelect :model-value="getSetFieldSelection(row)" @update:modelValue="(value) => handleSetFieldChange(row, value as any)">
                          <option v-for="option in automationSetFieldOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
                        </UiSelect>
                        <UiInput v-if="row.scope === 'custom_field'" v-model="row.customFieldKey" maxlength="80" placeholder="Custom field key" />
                        <UiInput
                          v-model="row.value"
                          maxlength="240"
                          :placeholder="row.scope === 'custom_field' ? 'YAML scalar value' : 'Value'"
                          :list="setFieldValueSuggestions(row).length ? `set-suggestions-${row.id}` : undefined"
                        />
                        <datalist v-if="setFieldValueSuggestions(row).length" :id="`set-suggestions-${row.id}`">
                          <option v-for="s in setFieldValueSuggestions(row)" :key="s" :value="s" />
                        </datalist>
                        <UiButton variant="danger" type="button" @click="removeSetField(activeCustomAction.event, row.id)">Remove</UiButton>
                      </div>
                    </div>
                    <div class="row" style="gap: 8px; margin-top: 10px;">
                      <UiButton variant="ghost" type="button" @click="addSetField(activeCustomAction.event)">Add set field</UiButton>
                    </div>
                  </div>

                  <div class="field-grid">
                    <div class="field">
                      <label class="field-label">Add values</label>
                      <div class="automation-builder__rows">
                        <div v-for="row in activeCustomAction.addFields" :key="row.id" class="automation-builder__row automation-builder__row--list">
                          <UiSelect v-model="row.field">
                            <option v-for="option in automationListFieldOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
                          </UiSelect>
                          <UiInput v-model="row.value" maxlength="240" :placeholder="listFieldPlaceholder(row.field)" />
                          <UiButton variant="danger" type="button" @click="removeListField(activeCustomAction.event, 'add', row.id)">Remove</UiButton>
                        </div>
                      </div>
                      <div class="row" style="gap: 8px; margin-top: 10px;">
                        <UiButton variant="ghost" type="button" @click="addListField(activeCustomAction.event, 'add')">Add add-row</UiButton>
                      </div>
                    </div>

                    <div class="field">
                      <label class="field-label">Remove values</label>
                      <div class="automation-builder__rows">
                        <div v-for="row in activeCustomAction.removeFields" :key="row.id" class="automation-builder__row automation-builder__row--list">
                          <UiSelect v-model="row.field">
                            <option v-for="option in automationListFieldOptions" :key="option.value" :value="option.value">{{ option.label }}</option>
                          </UiSelect>
                          <UiInput v-model="row.value" maxlength="240" :placeholder="listFieldPlaceholder(row.field)" />
                          <UiButton variant="danger" type="button" @click="removeListField(activeCustomAction.event, 'remove', row.id)">Remove</UiButton>
                        </div>
                      </div>
                      <div class="row" style="gap: 8px; margin-top: 10px;">
                        <UiButton variant="ghost" type="button" @click="addListField(activeCustomAction.event, 'remove')">Add remove-row</UiButton>
                      </div>
                    </div>
                  </div>

                  <div class="field">
                    <label class="field-label">Comment</label>
                    <textarea v-model="activeCustomAction.comment" class="automation-builder__textarea" rows="3" placeholder="Optional comment template"></textarea>
                  </div>

                  <div class="field">
                    <label class="field-label">Run command</label>
                    <UiSelect v-model="activeCustomAction.runMode">
                      <option value="none">No command</option>
                      <option value="shell">Shell command</option>
                      <option value="command">Structured command</option>
                    </UiSelect>
                  </div>

                  <div v-if="activeCustomAction.runMode === 'shell'" class="field">
                    <label class="field-label">Shell command</label>
                    <textarea v-model="activeCustomAction.runShell" class="automation-builder__textarea" rows="3" placeholder="sh -c 'echo $LOTAR_TICKET_ID'"></textarea>
                  </div>

                  <div v-if="activeCustomAction.runMode === 'command'" class="col" style="gap: 14px;">
                    <div class="field-grid">
                      <div class="field">
                        <label class="field-label">Command</label>
                        <UiInput v-model="activeCustomAction.runCommand" maxlength="200" placeholder="lotar" />
                      </div>
                      <div class="field">
                        <label class="field-label">Args</label>
                        <UiInput v-model="activeCustomAction.runArgs" maxlength="240" placeholder="Comma separated args" />
                      </div>
                      <div class="field">
                        <label class="field-label">Working directory</label>
                        <UiInput v-model="activeCustomAction.runCwd" maxlength="240" placeholder="Optional cwd" />
                      </div>
                    </div>
                    <div class="automation-builder__checkbox-row">
                      <label><input v-model="activeCustomAction.runWait" type="checkbox"> Wait for command</label>
                      <label><input v-model="activeCustomAction.runIgnoreFailure" type="checkbox"> Ignore failures</label>
                    </div>
                    <div class="field">
                      <label class="field-label">Extra environment</label>
                      <div class="automation-builder__rows">
                        <div v-for="env in activeCustomAction.runEnv" :key="env.id" class="automation-builder__row automation-builder__row--env">
                          <UiInput v-model="env.key" maxlength="80" placeholder="ENV_NAME" />
                          <UiInput v-model="env.value" maxlength="240" placeholder="Value" />
                          <UiButton variant="danger" type="button" @click="removeRunEnv(activeCustomAction.event, env.id)">Remove</UiButton>
                        </div>
                      </div>
                      <div class="row" style="gap: 8px; margin-top: 10px;">
                        <UiButton variant="ghost" type="button" @click="addRunEnv(activeCustomAction.event)">Add env var</UiButton>
                      </div>
                    </div>
                  </div>
                </div>
              </UiCard>
            </template>
          </section>

          <section v-else class="col" style="gap: 16px;">
            <UiCard>
              <div class="col" style="gap: 12px;">
                <div class="row" style="gap: 8px; align-items: center; flex-wrap: wrap;">
                  <strong>{{ dialogPreviewSummary.name || 'New rule' }}</strong>
                  <span v-for="event in dialogPreviewSummary.events" :key="event" class="automation-builder__event-chip">{{ event }}</span>
                </div>
                <p class="automation-builder__review-sentence">{{ dialogReviewSentence }}</p>
                <div class="automation-builder__summary-block" v-if="dialogPreviewSummary.conditions.length">
                  <span class="muted">Filters</span>
                  <div class="automation-builder__summary-list">
                    <span v-for="condition in dialogPreviewSummary.conditions" :key="condition" class="automation-builder__summary-chip">{{ condition }}</span>
                  </div>
                </div>
                <div class="automation-builder__summary-block">
                  <span class="muted">Results</span>
                  <div class="automation-builder__summary-list">
                    <span v-for="action in dialogPreviewSummary.actions" :key="action" class="automation-builder__summary-chip automation-builder__summary-chip--action">{{ action }}</span>
                  </div>
                </div>
              </div>
            </UiCard>

            <details class="automation-builder__preview-yaml">
              <summary>Generated YAML</summary>
              <div class="automation-builder__advanced-body" style="padding-top: 12px;">
                <textarea class="automation-builder__yaml automation-builder__yaml--readonly" rows="14" :value="dialogPreviewYaml" readonly></textarea>
              </div>
            </details>
          </section>
        </div>

        <footer class="automation-builder__dialog-actions">
          <UiButton variant="ghost" type="button" @click="dialogStep = Math.max(0, dialogStep - 1)" :disabled="dialogStep === 0">Back</UiButton>
          <div class="automation-builder__dialog-actions-right">
            <UiButton variant="ghost" type="button" @click="closeDialog">Cancel</UiButton>
            <UiButton v-if="dialogStep < stepLabels.length - 1" variant="primary" type="button" :disabled="!canAdvanceDialog" @click="advanceDialog">Next</UiButton>
            <UiButton v-else variant="primary" type="button" :disabled="!dialogCanSave" @click="commitDialogRule">{{ editingIndex === null ? 'Create rule' : 'Save rule' }}</UiButton>
          </div>
        </footer>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import {
    automationConditionFieldOptions,
    automationConditionOperatorOptions,
    automationEventOptions,
    automationListFieldOptions,
    automationSetFieldOptions,
    buildAutomationRuleFromDraft,
    buildAutomationYamlDocument,
    createChangeDraft,
    createEmptyRuleDraft,
    createListFieldDraft,
    createRunEnvDraft,
    createSetFieldDraft,
    formatAutomationRuleLabel,
    getAutomationEventLabel,
    getConditionFieldSelection,
    getEditableRuleState,
    getSetFieldSelection,
    normalizeConditionFieldSelection,
    normalizeSetFieldSelection,
    parseAutomationYamlDocument,
    summarizeAutomationRule,
    type AutomationConditionDraft,
    type AutomationEventActionDraft,
    type AutomationEventKey,
    type AutomationRuleDraft,
    type AutomationRunEnvDraft,
    type AutomationSetFieldDraft,
} from '../utils/automationRules'
import ChipListField from './ChipListField.vue'
import IconGlyph from './IconGlyph.vue'
import UiButton from './UiButton.vue'
import UiCard from './UiCard.vue'
import UiEmptyState from './UiEmptyState.vue'
import UiInput from './UiInput.vue'
import UiSelect from './UiSelect.vue'

type GuidedRecipe = 'status' | 'assignment' | 'tags' | 'comment' | 'command' | 'custom'

type GuidedRecipeOption = {
  value: GuidedRecipe
  label: string
  description: string
  example: string
}

const recipeOptions: GuidedRecipeOption[] = [
  {
    value: 'status',
    label: 'Move a task',
    description: 'Update the task status when a trigger happens.',
    example: 'Example: move to Review when a job completes.',
  },
  {
    value: 'assignment',
    label: 'Assign or hand off',
    description: 'Set the assignee or reporter automatically.',
    example: 'Example: hand the task back to the reporter when work finishes.',
  },
  {
    value: 'tags',
    label: 'Manage tags',
    description: 'Add or remove tags to keep workflow markers current.',
    example: 'Example: add ready-for-review after testing passes.',
  },
  {
    value: 'comment',
    label: 'Leave a comment',
    description: 'Write a note on the task when something happens.',
    example: 'Example: post “Ready for review” after a status change.',
  },
  {
    value: 'command',
    label: 'Run a command',
    description: 'Trigger a shell or structured command on the server.',
    example: 'Example: clean up an agent worktree when a job completes.',
  },
  {
    value: 'custom',
    label: 'Custom / power-user',
    description: 'Use multiple triggers or combine several actions.',
    example: 'Best when the common guided paths are too narrow.',
  },
]

const eventDetails: Record<AutomationEventKey, string> = {
  start: 'When a task change matches your filters.',
  created: 'When a task is first created.',
  updated: 'When a task is updated.',
  assigned: 'When assignment changes.',
  commented: 'When someone adds a comment.',
  sprint_changed: 'When sprint membership changes.',
  job_start: 'When an agent job begins.',
  complete: 'When an agent job succeeds.',
  error: 'When an agent job fails.',
  cancel: 'When an agent job is cancelled.',
}

const taskChangeEvents = new Set<AutomationEventKey>(['start', 'updated', 'assigned', 'sprint_changed'])

const props = withDefaults(defineProps<{
  modelValue: string
  effectiveYaml?: string
  loading?: boolean
  error?: string | null
  sourceLabel?: string
  scopeHint?: string
  editable?: boolean
  availableStatuses?: string[]
  availablePriorities?: string[]
  availableTypes?: string[]
  availableTags?: string[]
  availableMembers?: string[]
}>(), {
  effectiveYaml: '',
  loading: false,
  error: null,
  sourceLabel: '',
  scopeHint: '',
  editable: true,
  availableStatuses: () => [],
  availablePriorities: () => [],
  availableTypes: () => [],
  availableTags: () => [],
  availableMembers: () => [],
})

const emit = defineEmits<{
  (e: 'update:modelValue', value: string): void
}>()

const parsedDocument = computed(() => parseAutomationYamlDocument(props.modelValue))
const hasParseError = computed(() => Boolean(parsedDocument.value.parseError))

const builderRules = computed(() => parsedDocument.value.rules.map((rule, index) => {
  const editableState = getEditableRuleState(rule)
  return {
    id: `${index}-${rule.name || 'rule'}`,
    index,
    rule,
    summary: summarizeAutomationRule(rule),
    draft: editableState.draft,
    unsupportedReasons: editableState.unsupportedReasons,
  }
}))

const maxIterationsInput = computed(() => parsedDocument.value.maxIterations)

const dialogOpen = ref(false)
const dialogStep = ref(0)
const editingIndex = ref<number | null>(null)
const dialogDraft = ref(createBlankDraft())
const dialogRecipe = ref<GuidedRecipe | null>(null)
const selectedSimpleEvent = ref<AutomationEventKey | null>(null)
const activeCustomEvent = ref<AutomationEventKey | null>(null)
const simpleStatusValue = ref('')
const simpleAssignmentField = ref<'assignee' | 'reporter'>('assignee')
const simpleAssignmentValue = ref('')
const simpleTagMode = ref<'add' | 'remove'>('add')
const simpleTagValues = ref<string[]>([])
const simpleCommentValue = ref('')
const simpleCommandMode = ref<'shell' | 'command'>('shell')
const simpleCommandShell = ref('')
const simpleCommandSpec = ref({
  command: '',
  args: '',
  cwd: '',
  wait: true,
  ignoreFailure: false,
  env: [] as AutomationRunEnvDraft[],
})

const stepLabels = ['Goal', 'Trigger', 'Result', 'Review']

const memberSuggestions = computed(() => {
  const tokens = ['@assignee', '@reporter', '@assignee_or_reporter', '@me']
  return [...tokens, ...props.availableMembers.filter((m) => !tokens.includes(m))]
})

const visibleConditions = computed(() => dialogDraft.value.conditions.filter((row) => hasConditionContent(row)))
const visibleChangeConditions = computed(() => dialogDraft.value.changeConditions.filter((row) => hasChangeContent(row)))
const customEnabledEventActions = computed(() => dialogDraft.value.eventActions.filter((entry) => entry.enabled))
const activeCustomAction = computed(() => {
  if (dialogRecipe.value !== 'custom' || !activeCustomEvent.value) return null
  return dialogDraft.value.eventActions.find((entry) => entry.event === activeCustomEvent.value) ?? null
})

const workingDraft = computed(() => {
  if (dialogRecipe.value === 'custom') {
    return cloneDraft(dialogDraft.value)
  }
  return buildGuidedDraft()
})

const dialogPreviewRule = computed(() => {
  if (!dialogRecipe.value) return {}
  return buildAutomationRuleFromDraft(workingDraft.value)
})
const dialogPreviewYaml = computed(() => buildAutomationYamlDocument(Object.keys(dialogPreviewRule.value).length ? [dialogPreviewRule.value] : []))
const dialogPreviewSummary = computed(() => summarizeAutomationRule(dialogPreviewRule.value))
const dialogCanSave = computed(() => {
  if (!dialogRecipe.value) return false
  return hasDraftActionContent(workingDraft.value)
})
const canAdvanceDialog = computed(() => {
  if (dialogStep.value === 0) return Boolean(dialogRecipe.value)
  if (dialogStep.value === 1) {
    if (dialogRecipe.value === 'custom') return customEnabledEventActions.value.length > 0
    return Boolean(selectedSimpleEvent.value)
  }
  if (dialogStep.value === 2) return dialogCanSave.value
  return true
})
const canUseChangeConditions = computed(() => {
  if (dialogRecipe.value === 'custom') {
    return customEnabledEventActions.value.some((entry) => taskChangeEvents.has(entry.event))
  }
  return selectedSimpleEvent.value ? taskChangeEvents.has(selectedSimpleEvent.value) : false
})
const dialogReviewSentence = computed(() => {
  if (!dialogRecipe.value) {
    return 'Choose a goal, trigger, and result to preview the rule.'
  }
  const trigger = dialogPreviewSummary.value.events[0]?.toLowerCase() || 'this trigger'
  const action = firstDialogActionLine()
  const conditions = dialogPreviewSummary.value.conditions.length
    ? ` if ${dialogPreviewSummary.value.conditions.join(' and ')}`
    : ''
  return `When ${trigger}${conditions}, ${action}.`
})

watch(customEnabledEventActions, (entries) => {
  if (dialogRecipe.value !== 'custom') return
  if (!entries.length) {
    activeCustomEvent.value = null
    return
  }
  if (!activeCustomEvent.value || !entries.some((entry) => entry.event === activeCustomEvent.value)) {
    activeCustomEvent.value = entries[0].event
  }
}, { deep: true })

function updateYaml(value: string) {
  emit('update:modelValue', value)
}

function updateMaxIterations(value: string) {
  emit('update:modelValue', buildAutomationYamlDocument(parsedDocument.value.rules, value))
}

function openCreateDialog() {
  dialogDraft.value = createBlankDraft()
  dialogRecipe.value = null
  selectedSimpleEvent.value = null
  activeCustomEvent.value = null
  resetSimpleState()
  dialogStep.value = 0
  editingIndex.value = null
  dialogOpen.value = true
}

function openEditDialog(index: number) {
  const draft = getEditableRuleState(parsedDocument.value.rules[index]).draft
  dialogDraft.value = prepareDraftForDialog(draft)
  dialogStep.value = 0
  editingIndex.value = index
  dialogRecipe.value = inferRecipe(dialogDraft.value)
  if (dialogRecipe.value === 'custom') {
    activeCustomEvent.value = dialogDraft.value.eventActions.find((entry) => entry.enabled)?.event ?? null
  } else {
    populateSimpleStateFromDraft(dialogDraft.value, dialogRecipe.value)
  }
  dialogOpen.value = true
}

function closeDialog() {
  dialogOpen.value = false
}

function deleteRule(index: number) {
  const nextRules = [...parsedDocument.value.rules]
  nextRules.splice(index, 1)
  emit('update:modelValue', buildAutomationYamlDocument(nextRules, parsedDocument.value.maxIterations))
}

function duplicateRule(index: number) {
  const nextRules = [...parsedDocument.value.rules]
  const clone = JSON.parse(JSON.stringify(nextRules[index]))
  nextRules.splice(index + 1, 0, clone)
  emit('update:modelValue', buildAutomationYamlDocument(nextRules, parsedDocument.value.maxIterations))
}

function moveRule(index: number, direction: number) {
  const target = index + direction
  if (target < 0 || target >= parsedDocument.value.rules.length) return
  const nextRules = [...parsedDocument.value.rules]
  const [rule] = nextRules.splice(index, 1)
  nextRules.splice(target, 0, rule)
  emit('update:modelValue', buildAutomationYamlDocument(nextRules, parsedDocument.value.maxIterations))
}

function advanceDialog() {
  if (!canAdvanceDialog.value) return
  dialogStep.value = Math.min(stepLabels.length - 1, dialogStep.value + 1)
}

function commitDialogRule() {
  if (!dialogCanSave.value) return
  const nextRules = [...parsedDocument.value.rules]
  const nextRule = buildAutomationRuleFromDraft(workingDraft.value)
  if (editingIndex.value === null) {
    nextRules.push(nextRule)
  } else {
    nextRules.splice(editingIndex.value, 1, nextRule)
  }
  emit('update:modelValue', buildAutomationYamlDocument(nextRules, parsedDocument.value.maxIterations))
  closeDialog()
}

function selectRecipe(recipe: GuidedRecipe) {
  if (dialogRecipe.value !== recipe && dialogRecipe.value && dialogRecipe.value !== 'custom' && recipe === 'custom') {
    dialogDraft.value = cloneDraft(workingDraft.value)
  }
  dialogRecipe.value = recipe
  if (recipe === 'custom') {
    activeCustomEvent.value = dialogDraft.value.eventActions.find((entry) => entry.enabled)?.event ?? activeCustomEvent.value
    return
  }
  if (selectedSimpleEvent.value == null) {
    selectedSimpleEvent.value = recommendedEventForRecipe(recipe)
  }
  populateSimpleStateFromDraft(dialogDraft.value, recipe)
}

function addCondition() {
  dialogDraft.value.conditions.push(createEmptyRuleDraft().conditions[0])
}

function removeCondition(id: string) {
  dialogDraft.value.conditions = dialogDraft.value.conditions.filter((row) => row.id !== id)
}

function addChangeCondition() {
  dialogDraft.value.changeConditions.push(createChangeDraft())
}

function removeChangeCondition(id: string) {
  dialogDraft.value.changeConditions = dialogDraft.value.changeConditions.filter((row) => row.id !== id)
}

function addSetField(event: string) {
  const action = findDraftEventAction(event)
  if (!action) return
  action.setFields.push(createSetFieldDraft())
}

function removeSetField(event: string, id: string) {
  const action = findDraftEventAction(event)
  if (!action) return
  action.setFields = action.setFields.filter((row) => row.id !== id)
}

function addListField(event: string, kind: 'add' | 'remove') {
  const action = findDraftEventAction(event)
  if (!action) return
  action[kind === 'add' ? 'addFields' : 'removeFields'].push(createListFieldDraft())
}

function removeListField(event: string, kind: 'add' | 'remove', id: string) {
  const action = findDraftEventAction(event)
  if (!action) return
  const key = kind === 'add' ? 'addFields' : 'removeFields'
  action[key] = action[key].filter((row) => row.id !== id)
}

function addRunEnv(event: string) {
  const action = findDraftEventAction(event)
  if (!action) return
  action.runEnv.push(createRunEnvDraft())
}

function removeRunEnv(event: string, id: string) {
  const action = findDraftEventAction(event)
  if (!action) return
  action.runEnv = action.runEnv.filter((row) => row.id !== id)
}

function addSimpleCommandEnv() {
  simpleCommandSpec.value.env.push(createRunEnvDraft())
}

function removeSimpleCommandEnv(id: string) {
  simpleCommandSpec.value.env = simpleCommandSpec.value.env.filter((row) => row.id !== id)
}

function isCustomEventEnabled(event: AutomationEventKey) {
  return Boolean(dialogDraft.value.eventActions.find((entry) => entry.event === event)?.enabled)
}

function toggleCustomEvent(event: AutomationEventKey) {
  const action = findDraftEventAction(event)
  if (!action) return
  action.enabled = !action.enabled
}

function findDraftEventAction(event: string): AutomationEventActionDraft | undefined {
  return dialogDraft.value.eventActions.find((entry) => entry.event === event)
}

function handleConditionFieldChange(row: AutomationConditionDraft, selection: any) {
  normalizeConditionFieldSelection(row, selection)
}

function handleSetFieldChange(row: AutomationSetFieldDraft, selection: any) {
  normalizeSetFieldSelection(row, selection)
}

function describeRuleCard(summary: ReturnType<typeof summarizeAutomationRule>) {
  const trigger = summary.events[0]?.toLowerCase() || 'the trigger matches'
  const firstAction = stripLeadingEvent(summary.actions[0])
  const nextAction = summary.actions.length > 1 ? ` and ${stripLeadingEvent(summary.actions[1])}` : ''
  const filters = summary.conditions.length ? ` when ${summary.conditions.slice(0, 2).join(' and ')}` : ''
  return `Runs on ${trigger}${filters} and will ${firstAction}${nextAction}.`
}

function firstDialogActionLine() {
  if (dialogRecipe.value === 'status') {
    return simpleStatusValue.value.trim() ? `set status to ${simpleStatusValue.value.trim()}` : 'set a status'
  }
  if (dialogRecipe.value === 'assignment') {
    return simpleAssignmentValue.value.trim()
      ? `set ${simpleAssignmentField.value} to ${simpleAssignmentValue.value.trim()}`
      : `set ${simpleAssignmentField.value}`
  }
  if (dialogRecipe.value === 'tags') {
    return simpleTagValues.value.length
      ? `${simpleTagMode.value} tags ${simpleTagValues.value.join(', ')}`
      : `${simpleTagMode.value} tags`
  }
  if (dialogRecipe.value === 'comment') {
    return 'add a comment'
  }
  if (dialogRecipe.value === 'command') {
    return simpleCommandMode.value === 'shell'
      ? 'run a shell command'
      : (simpleCommandSpec.value.command.trim() ? `run ${simpleCommandSpec.value.command.trim()}` : 'run a command')
  }
  return stripLeadingEvent(dialogPreviewSummary.value.actions[0]) || 'apply the configured actions'
}

function stripLeadingEvent(line?: string) {
  if (!line) return 'apply the configured actions'
  const index = line.indexOf(': ')
  return index === -1 ? line : line.slice(index + 2)
}

function buildGuidedDraft(): AutomationRuleDraft {
  const base = cloneDraft(dialogDraft.value)
  base.conditions = visibleConditions.value.map((row) => ({ ...row }))
  base.changeConditions = visibleChangeConditions.value.map((row) => ({ ...row }))
  base.eventActions = automationEventOptions.map((option) => emptyEventAction(option.value))

  if (!dialogRecipe.value || dialogRecipe.value === 'custom' || !selectedSimpleEvent.value) {
    return base
  }

  const action = base.eventActions.find((entry) => entry.event === selectedSimpleEvent.value)
  if (!action) return base
  action.enabled = true

  if (dialogRecipe.value === 'status') {
    const value = simpleStatusValue.value.trim()
    if (value) {
      action.setFields = [createGuidedSetField('status', value)]
    }
    return base
  }

  if (dialogRecipe.value === 'assignment') {
    const value = simpleAssignmentValue.value.trim()
    if (value) {
      action.setFields = [createGuidedSetField(simpleAssignmentField.value, value)]
    }
    return base
  }

  if (dialogRecipe.value === 'tags') {
    if (simpleTagValues.value.length) {
      const row = createListFieldDraft('tags')
      row.value = simpleTagValues.value.join(', ')
      if (simpleTagMode.value === 'add') {
        action.addFields = [row]
      } else {
        action.removeFields = [row]
      }
    }
    return base
  }

  if (dialogRecipe.value === 'comment') {
    action.comment = simpleCommentValue.value
    return base
  }

  if (dialogRecipe.value === 'command') {
    if (simpleCommandMode.value === 'shell') {
      action.runMode = 'shell'
      action.runShell = simpleCommandShell.value
    } else {
      action.runMode = 'command'
      action.runCommand = simpleCommandSpec.value.command
      action.runArgs = simpleCommandSpec.value.args
      action.runCwd = simpleCommandSpec.value.cwd
      action.runWait = simpleCommandSpec.value.wait
      action.runIgnoreFailure = simpleCommandSpec.value.ignoreFailure
      action.runEnv = simpleCommandSpec.value.env.map((env) => ({ ...env }))
    }
  }

  return base
}

function inferRecipe(draft: AutomationRuleDraft): GuidedRecipe {
  const enabled = draft.eventActions.filter((entry) => entry.enabled)
  if (enabled.length !== 1) return 'custom'

  const entry = enabled[0]
  const setFields = entry.setFields.filter((row) => row.value.trim())
  const addFields = entry.addFields.filter((row) => row.value.trim())
  const removeFields = entry.removeFields.filter((row) => row.value.trim())
  const hasComment = Boolean(entry.comment.trim())
  const hasRun = (entry.runMode === 'shell' && Boolean(entry.runShell.trim())) || (entry.runMode === 'command' && Boolean(entry.runCommand.trim()))

  if (hasComment && !setFields.length && !addFields.length && !removeFields.length && !hasRun) return 'comment'
  if (hasRun && !setFields.length && !addFields.length && !removeFields.length && !hasComment) return 'command'
  if (
    setFields.length === 1
    && !addFields.length
    && !removeFields.length
    && !hasComment
    && !hasRun
    && setFields[0].scope === 'field'
    && setFields[0].field === 'status'
  ) {
    return 'status'
  }
  if (
    setFields.length === 1
    && !addFields.length
    && !removeFields.length
    && !hasComment
    && !hasRun
    && setFields[0].scope === 'field'
    && (setFields[0].field === 'assignee' || setFields[0].field === 'reporter')
  ) {
    return 'assignment'
  }
  if (
    !setFields.length
    && !hasComment
    && !hasRun
    && addFields.concat(removeFields).every((row) => row.field === 'tags')
    && addFields.concat(removeFields).length > 0
  ) {
    return 'tags'
  }
  return 'custom'
}

function populateSimpleStateFromDraft(draft: AutomationRuleDraft, recipe: GuidedRecipe) {
  resetSimpleState()
  const enabled = draft.eventActions.filter((entry) => entry.enabled)
  const entry = enabled[0]
  selectedSimpleEvent.value = entry?.event ?? recommendedEventForRecipe(recipe)
  if (!entry) return

  if (recipe === 'status') {
    simpleStatusValue.value = entry.setFields.find((row) => row.field === 'status')?.value ?? ''
    return
  }

  if (recipe === 'assignment') {
    const field = entry.setFields.find((row) => row.field === 'assignee' || row.field === 'reporter')
    simpleAssignmentField.value = field?.field === 'reporter' ? 'reporter' : 'assignee'
    simpleAssignmentValue.value = field?.value ?? ''
    return
  }

  if (recipe === 'tags') {
    const source = entry.addFields.find((row) => row.field === 'tags') ?? entry.removeFields.find((row) => row.field === 'tags')
    simpleTagMode.value = entry.addFields.some((row) => row.field === 'tags' && row.value.trim()) ? 'add' : 'remove'
    simpleTagValues.value = splitCsv(source?.value ?? '')
    return
  }

  if (recipe === 'comment') {
    simpleCommentValue.value = entry.comment
    return
  }

  if (recipe === 'command') {
    if (entry.runMode === 'command') {
      simpleCommandMode.value = 'command'
      simpleCommandSpec.value = {
        command: entry.runCommand,
        args: entry.runArgs,
        cwd: entry.runCwd,
        wait: entry.runWait,
        ignoreFailure: entry.runIgnoreFailure,
        env: entry.runEnv.map((env) => ({ ...env })),
      }
    } else {
      simpleCommandMode.value = 'shell'
      simpleCommandShell.value = entry.runShell
    }
  }
}

function resetSimpleState() {
  simpleStatusValue.value = ''
  simpleAssignmentField.value = 'assignee'
  simpleAssignmentValue.value = ''
  simpleTagMode.value = 'add'
  simpleTagValues.value = []
  simpleCommentValue.value = ''
  simpleCommandMode.value = 'shell'
  simpleCommandShell.value = ''
  simpleCommandSpec.value = {
    command: '',
    args: '',
    cwd: '',
    wait: true,
    ignoreFailure: false,
    env: [],
  }
}

function createBlankDraft() {
  const draft = createEmptyRuleDraft()
  draft.conditions = []
  draft.changeConditions = []
  return draft
}

function prepareDraftForDialog(draft: AutomationRuleDraft) {
  const next = cloneDraft(draft)
  next.conditions = next.conditions.filter((row) => hasConditionContent(row))
  next.changeConditions = next.changeConditions.filter((row) => hasChangeContent(row))
  return next
}

function createGuidedSetField(field: string, value: string) {
  const row = createSetFieldDraft()
  row.field = field
  row.value = value
  return row
}

function emptyEventAction(event: AutomationEventKey): AutomationEventActionDraft {
  return {
    event,
    enabled: false,
    comment: '',
    setFields: [],
    addFields: [],
    removeFields: [],
    runMode: 'none',
    runShell: '',
    runCommand: '',
    runArgs: '',
    runCwd: '',
    runWait: true,
    runIgnoreFailure: false,
    runEnv: [],
  }
}

function hasDraftActionContent(draft: AutomationRuleDraft) {
  return draft.eventActions.some((entry) => {
    if (!entry.enabled) return false
    return Boolean(
      entry.comment.trim()
      || entry.setFields.some((row) => row.value.trim())
      || entry.addFields.some((row) => row.value.trim())
      || entry.removeFields.some((row) => row.value.trim())
      || (entry.runMode === 'shell' && entry.runShell.trim())
      || (entry.runMode === 'command' && entry.runCommand.trim()),
    )
  })
}

function recommendedEventForRecipe(recipe: GuidedRecipe): AutomationEventKey | null {
  switch (recipe) {
    case 'status':
      return 'complete'
    case 'assignment':
      return 'complete'
    case 'tags':
      return 'updated'
    case 'comment':
      return 'complete'
    case 'command':
      return 'complete'
    default:
      return null
  }
}

function conditionValuePlaceholder(operator: string) {
  switch (operator) {
    case 'in':
    case 'any':
    case 'all':
    case 'none':
      return 'Comma separated values'
    case 'before':
      return 'today or YYYY-MM-DD'
    case 'within':
    case 'older_than':
      return '3d, 1w, 2m'
    case 'matches':
      return 'Regex pattern'
    default:
      return 'Value'
  }
}

function conditionValueSuggestions(row: AutomationConditionDraft): string[] {
  if (row.scope === 'custom_field') return []
  switch (row.field) {
    case 'status': return props.availableStatuses
    case 'priority': return props.availablePriorities
    case 'type': return props.availableTypes
    case 'tags': return props.availableTags
    case 'assignee':
    case 'reporter': return props.availableMembers
    default: return []
  }
}

function setFieldValueSuggestions(row: AutomationSetFieldDraft): string[] {
  if (row.scope === 'custom_field') return []
  switch (row.field) {
    case 'status': return props.availableStatuses
    case 'priority': return props.availablePriorities
    case 'type': return props.availableTypes
    case 'assignee':
    case 'reporter': return memberSuggestions.value
    default: return []
  }
}

function listFieldPlaceholder(field: string) {
  if (field === 'sprint') return 'Sprint label or id'
  return 'Comma separated values'
}

function hasConditionContent(row: AutomationConditionDraft) {
  if (row.scope === 'custom_field' && row.customFieldKey.trim()) return true
  if (row.field.trim()) return true
  return row.value.trim().length > 0
}

function hasChangeContent(row: { field: string; from: string; to: string }) {
  return Boolean(row.field.trim() || row.from.trim() || row.to.trim())
}

function splitCsv(value: string) {
  return value
    .split(',')
    .map((entry) => entry.trim())
    .filter(Boolean)
}

function cloneDraft(draft: AutomationRuleDraft): AutomationRuleDraft {
  return JSON.parse(JSON.stringify(draft)) as AutomationRuleDraft
}
</script>

<style scoped>
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
  padding: 16px;
}

.automation-builder__rule-head {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.automation-builder__rule-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

.automation-builder__rule-sentence,
.automation-builder__review-sentence {
  margin: 0;
  font-size: 14px;
  line-height: 1.5;
}

.automation-builder__rule-note {
  padding: 10px 12px;
  border-radius: 12px;
  background: color-mix(in srgb, var(--color-warning, #d97706) 10%, transparent);
  color: var(--color-muted);
  font-size: 13px;
}

.automation-builder__event-chip,
.automation-builder__meta-chip,
.automation-builder__warning-chip,
.automation-builder__summary-chip {
  display: inline-flex;
  align-items: center;
  padding: 4px 10px;
  border-radius: 999px;
  font-size: 12px;
  border: 1px solid var(--color-border);
  background: var(--color-surface-2, rgba(0, 0, 0, 0.03));
}

.automation-builder__warning-chip {
  background: color-mix(in srgb, var(--color-warning, #d97706) 12%, transparent);
  border-color: color-mix(in srgb, var(--color-warning, #d97706) 36%, var(--color-border));
}

.automation-builder__summary-chip--action {
  background: color-mix(in srgb, var(--color-accent, #0f766e) 12%, transparent);
}

.automation-builder__summary-block {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.automation-builder__summary-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.automation-builder__advanced,
.automation-builder__preview-yaml {
  border: 1px solid var(--color-border);
  border-radius: 14px;
  background: var(--color-surface-2, rgba(0, 0, 0, 0.02));
}

.automation-builder__advanced summary,
.automation-builder__preview-yaml summary {
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

.automation-builder__yaml,
.automation-builder__textarea {
  width: 100%;
  border: 1px solid var(--color-border);
  border-radius: 12px;
  background: var(--color-surface, #fff);
  padding: 12px 14px;
  font: inherit;
  resize: vertical;
}

.automation-builder__yaml--readonly {
  opacity: 0.85;
}

.automation-builder__dialog-backdrop {
  position: fixed;
  inset: 0;
  background: rgba(15, 23, 42, 0.46);
  backdrop-filter: blur(6px);
  display: flex;
  align-items: center;
  justify-content: center;
  padding: 24px;
  z-index: 40;
}

.automation-builder__dialog {
  width: min(980px, 100%);
  max-height: calc(100vh - 48px);
  overflow: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 18px;
}

.automation-builder__dialog-header {
  display: flex;
  justify-content: space-between;
  gap: 16px;
  align-items: flex-start;
}

.automation-builder__dialog-header h2 {
  margin: 0 0 4px;
}

.automation-builder__dialog-header p {
  margin: 0;
}

.automation-builder__stepper {
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 8px;
}

.automation-builder__step {
  border: 1px solid var(--color-border);
  background: var(--color-surface-2, rgba(0, 0, 0, 0.02));
  border-radius: 14px;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.automation-builder__step--active {
  border-color: color-mix(in srgb, var(--color-accent, #0f766e) 45%, var(--color-border));
  background: color-mix(in srgb, var(--color-accent, #0f766e) 12%, transparent);
}

.automation-builder__step--complete {
  border-color: color-mix(in srgb, var(--color-success, #16a34a) 35%, var(--color-border));
}

.automation-builder__dialog-body {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.automation-builder__guide-block {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.automation-builder__guide-block h3 {
  margin: 0;
  font-size: 1rem;
}

.automation-builder__guide-block p {
  margin: 0;
}

.automation-builder__recipe-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
  gap: 12px;
}

.automation-builder__recipe-card,
.automation-builder__event-toggle,
.automation-builder__event-tab {
  border: 1px solid var(--color-border);
  border-radius: 14px;
  background: var(--color-surface-2, rgba(0, 0, 0, 0.02));
  padding: 14px;
  cursor: pointer;
  text-align: left;
}

.automation-builder__recipe-card {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.automation-builder__recipe-card p,
.automation-builder__recipe-card span {
  margin: 0;
}

.automation-builder__recipe-card--active,
.automation-builder__event-toggle--active,
.automation-builder__event-tab--active {
  border-color: color-mix(in srgb, var(--color-accent, #0f766e) 45%, var(--color-border));
  background: color-mix(in srgb, var(--color-accent, #0f766e) 12%, transparent);
}

.automation-builder__event-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 10px;
}

.automation-builder__event-toggle {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.automation-builder__event-toggle span {
  color: var(--color-muted);
  font-size: 13px;
}

.automation-builder__event-tabs {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.automation-builder__event-tab {
  padding: 10px 12px;
}

.automation-builder__inline-empty {
  padding: 12px 14px;
  border-radius: 12px;
  border: 1px dashed var(--color-border);
  color: var(--color-muted);
  background: var(--color-surface-2, rgba(0, 0, 0, 0.02));
}

.automation-builder__rows {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.automation-builder__row {
  display: grid;
  gap: 8px;
  align-items: center;
}

.automation-builder__row--condition {
  grid-template-columns: minmax(0, 1.1fr) minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.3fr) auto;
}

.automation-builder__row--change,
.automation-builder__row--set,
.automation-builder__row--env {
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr) minmax(0, 1.3fr) auto;
}

.automation-builder__row--list {
  grid-template-columns: minmax(0, 1fr) minmax(0, 1.6fr) auto;
}

.automation-builder__event-card {
  padding: 16px;
}

.automation-builder__checkbox-row {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
  font-size: 14px;
}

.automation-builder__dialog-actions {
  display: flex;
  justify-content: space-between;
  gap: 12px;
  align-items: center;
}

.automation-builder__dialog-actions-right {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  justify-content: flex-end;
}

@media (max-width: 900px) {
  .automation-builder__rule-head {
    flex-direction: column;
  }

  .automation-builder__rule-actions {
    justify-content: flex-start;
  }

  .automation-builder__stepper {
    grid-template-columns: repeat(2, minmax(0, 1fr));
  }

  .automation-builder__row--condition,
  .automation-builder__row--change,
  .automation-builder__row--set,
  .automation-builder__row--env,
  .automation-builder__row--list {
    grid-template-columns: minmax(0, 1fr);
  }
}

@media (max-width: 640px) {
  .automation-builder__dialog {
    padding: 16px;
  }

  .automation-builder__stepper {
    grid-template-columns: minmax(0, 1fr);
  }
}
</style>
