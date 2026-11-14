import { mount } from '@vue/test-utils'
import { describe, expect, it } from 'vitest'
import CommentReport from '../components/task-panel/CommentReport.vue'

describe('CommentReport (DEV-490)', () => {
    const report = {
        commits: [{ hash: '5b7385dc0123', what: 'DEV-490 Report comments' }],
        checks: [
            { command: 'npm test', result: 'passed' as const },
            { command: 'npm run lint', result: 'failed' as const, reason: 'clippy' },
        ],
        next: 'review agent',
    }

    it('links commits when the remote is known and shows each check result', () => {
        const wrapper = mount(CommentReport, { props: { report, commitUrl: 'https://github.com/o/r/commit/' } })
        const link = wrapper.get('a.comment-report__hash')
        expect(link.attributes('href')).toBe('https://github.com/o/r/commit/5b7385dc0123')
        expect(link.text()).toBe('5b7385dc')
        const checks = wrapper
            .findAll('.comment-report__check')
            .map((check) => [check.attributes('data-result'), check.text()])
        expect(checks).toEqual([
            ['passed', '✓npm testpassed'],
            ['failed', '✗npm run lintfailedclippy'],
        ])
        expect(wrapper.text()).toContain('Next: review agent')
    })

    it('shows a plain hash without a remote and says when no check is marked not run', () => {
        const wrapper = mount(CommentReport, { props: { report } })
        expect(wrapper.find('a').exists()).toBe(false)
        expect(wrapper.get('code.comment-report__hash').text()).toBe('5b7385dc')
        expect(wrapper.get('[data-testid="comment-report-missing"]').text()).toBe('No check marked not run')
    })

    it('says when a report lists no checks, and nothing when one did not run', () => {
        expect(mount(CommentReport, { props: { report: { next: 'x' } } }).text()).toContain('No checks listed')
        const ran = mount(CommentReport, {
            props: { report: { checks: [{ command: 'npm run smoke', result: 'not_run', reason: 'no browser' }] } },
        })
        expect(ran.find('[data-testid="comment-report-missing"]').exists()).toBe(false)
        expect(ran.text()).toContain('not run')
    })
})
