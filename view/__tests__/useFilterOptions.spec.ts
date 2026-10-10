import { effectScope, ref } from 'vue'
import { describe, expect, it } from 'vitest'
import type { TaskDTO } from '../api/types'
import { useFilterOptions } from '../composables/useFilterOptions'

function task(id: string, tags: string[], fields: Record<string, unknown> = {}, assignee = '') {
  return { id, tags, custom_fields: fields, assignee } as TaskDTO
}

describe('project-scoped filter option sources', () => {
  it('unions configured and observed options, omits wildcards, and retains known values after filtering', () => {
    const scope = effectScope()
    scope.run(() => {
      const tasks = ref([task('DEV-1', ['ui', 'release candidate'], { iteration: 'beta', count: 2, ready: true, nested: {}, literal: ' padded ', wildcard: '*' }, 'alice')])
      const options = useFilterOptions(() => tasks.value, () => 'DEV', {
        scope: () => 'DEV', tags: () => ['*', 'ui', 'configured'], members: () => ['alice', 'bob'],
      })
      expect(options.value.tags).toEqual(['configured', 'release candidate', 'ui'])
      expect(options.value.assignees).toEqual(['alice', 'bob'])
      expect(options.value.customFieldValues).toEqual({ iteration: ['beta'], count: ['2'], ready: ['true'], literal: [' padded '], wildcard: ['*'] })
      tasks.value = []
      expect(options.value.tags).toContain('release candidate')
      expect(options.value.customFieldValues.iteration).toEqual(['beta'])
    })
    scope.stop()
  })

  it('hides stale config and task rows immediately during a project switch and safely restores same-scope knowledge', () => {
    const scope = effectScope()
    scope.run(() => {
      const project = ref('DEV')
      const configScope = ref('DEV')
      const tasks = ref([task('DEV-1', ['dev-only'], { iteration: 'dev' })])
      const options = useFilterOptions(() => tasks.value, () => project.value, {
        scope: () => configScope.value, tags: () => ['configured-old'], members: () => ['old-member'],
      })
      project.value = 'OTHER'
      expect(options.value).toEqual({ tags: [], assignees: [], customFieldValues: {} })
      tasks.value = [task('OTHER-1', ['other-only'], { iteration: 'other' }), task('DEV-2', ['foreign'])]
      expect(options.value.tags).toEqual(['other-only'])
      expect(options.value.customFieldValues).toEqual({ iteration: ['other'] })
      project.value = 'DEV'
      expect(options.value.tags).toContain('dev-only')
      expect(options.value.tags).not.toContain('other-only')
      expect(options.value.customFieldValues.iteration).toEqual(['dev'])
    })
    scope.stop()
  })

  it('uses canonical hyphenated project prefixes rather than substring matching', () => {
    const scope = effectScope()
    scope.run(() => {
      const options = useFilterOptions(() => [task('MY-APP-1', ['own']), task('MY-1', ['foreign'])], () => 'MY-APP', {
        scope: () => '', tags: () => [], members: () => [],
      })
      expect(options.value.tags).toEqual(['own'])
    })
    scope.stop()
  })

  it('preserves literal object-property custom names without changing prototypes', () => {
    const scope = effectScope()
    scope.run(() => {
      const fields = Object.fromEntries([['constructor', 'build'], ['__proto__', 'safe']])
      const options = useFilterOptions(() => [task('DEV-1', [], fields)], () => 'DEV', {
        scope: () => 'DEV', tags: () => [], members: () => [],
      })
      expect(options.value.customFieldValues.constructor).toEqual(['build'])
      expect(Object.prototype.hasOwnProperty.call(options.value.customFieldValues, '__proto__')).toBe(true)
      expect(options.value.customFieldValues.__proto__).toEqual(['safe'])
    })
    scope.stop()
  })
})
