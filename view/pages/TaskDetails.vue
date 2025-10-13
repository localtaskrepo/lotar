<template>
  <div class="task-details-route" />
</template>

<script setup lang="ts">
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { useTaskPanelController } from '../composables/useTaskPanelController'

const route = useRoute()
const router = useRouter()
const { openTaskPanel, closeTaskPanel } = useTaskPanelController()

const suppressCloseNavigation = ref(false)
const skipNavigationOnClose = ref(false)

function openFromRoute() {
  const rawId = route.params.id
  if (rawId === undefined || rawId === null) {
    return
  }
  const taskId = String(rawId)
  const projectParam = route.query.project
  const initialProject = typeof projectParam === 'string' ? projectParam : null

  openTaskPanel({
    taskId,
    initialProject,
    onClose: () => {
      if (skipNavigationOnClose.value) {
        skipNavigationOnClose.value = false
        return
      }
      if (suppressCloseNavigation.value) {
        suppressCloseNavigation.value = false
        return
      }
      router.push('/')
    },
    onCreated: (task) => {
      suppressCloseNavigation.value = true
      router.replace(`/task/${encodeURIComponent(task.id)}`)
    },
  })
}

watch(
  () => [route.params.id, route.query.project],
  () => {
    openFromRoute()
  },
)

onMounted(() => {
  openFromRoute()
})

onBeforeUnmount(() => {
  skipNavigationOnClose.value = true
  closeTaskPanel()
})
</script>

<style scoped>
</style>
