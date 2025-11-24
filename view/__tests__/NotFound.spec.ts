import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import { createMemoryHistory, createRouter } from 'vue-router'
import NotFound from '../pages/NotFound.vue'

const buildRouter = () =>
    createRouter({
        history: createMemoryHistory(),
        routes: [{ path: '/:pathMatch(.*)*', component: NotFound }],
    })

describe('NotFound page', () => {
    it('shows the missing path and recovery actions', async () => {
        const router = buildRouter()
        await router.push('/missing/path')
        await router.isReady()
        const wrapper = mount(NotFound, { global: { plugins: [router] } })
        expect(wrapper.text()).toContain('This page took a coffee break')
        expect(wrapper.text()).toContain('/missing/path')
        expect(wrapper.text()).toContain('Back to tasks')
    })
})
