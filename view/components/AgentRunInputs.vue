<template>
  <div class="run-inputs">
    <UiButton
      variant="ghost"
      class="run-inputs__toggle"
      :aria-expanded="open ? 'true' : 'false'"
      :aria-controls="panelId"
      @click="toggle"
    >
      {{ open ? 'Hide instructions' : 'Instructions' }}
    </UiButton>
    <AgentRunInputsPanel v-if="seen" v-show="open" :id="panelId" :job-id="jobId" :open="open" />
  </div>
</template>

<script setup lang="ts">
import { ref, useId, watch } from 'vue'
import AgentRunInputsPanel from './AgentRunInputsPanel.vue'
import UiButton from './UiButton.vue'

// The Instructions button with its panel below it, for a job without a log to show them in.
defineProps<{ jobId: string }>()

const panelId = `run-inputs-${useId()}`
// Bound by a page that swaps this for the log's header button, so an open panel stays open.
const open = defineModel<boolean>('open', { default: false })
// The panel stays mounted once opened; it loads on its first open and again only while there is no record.
const seen = ref(false)
watch(open, (value) => (seen.value ||= value), { immediate: true })

function toggle() {
  open.value = !open.value
}
</script>

<style scoped>
.run-inputs {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.run-inputs__toggle {
  align-self: flex-start;
}
</style>
