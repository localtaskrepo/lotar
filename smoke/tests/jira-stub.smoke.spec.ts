import { describe, it } from 'vitest';
import { JiraStub } from '../helpers/jira-stub.js';

describe('owned Jira fixture socket allocation', () => {
    it('binds concurrent fixtures to distinct live ports and serves each scope', async ({ expect }) => {
        const stubs: JiraStub[] = [];
        try {
            const results = await Promise.allSettled(Array.from({ length: 8 }, async (_, index) => {
                const projectKey = `PORT${index}`;
                const stub = await JiraStub.start({
                    projectKey,
                    issues: [{ key: `${projectKey}-1`, summary: projectKey }],
                });
                stubs.push(stub);
                const response = await fetch(`${stub.url}/rest/api/3/search/jql`);
                expect(response.status).toBe(200);
                const data = await response.json() as { issues: Array<{ key: string }> };
                expect(data.issues.map(issue => issue.key)).toEqual([`${projectKey}-1`]);
            }));
            for (const result of results) {
                if (result.status === 'rejected') throw result.reason;
            }
            expect(new Set(stubs.map(stub => stub.url)).size).toBe(8);
        } finally {
            await Promise.all(stubs.map(stub => stub.stop()));
        }
    });
});
