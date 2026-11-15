import { computed, hasInjectionContext, inject } from 'vue'
import { routerKey, type Router } from 'vue-router'
import { formatFileLocation, parseFileLocation, type FileLocation } from '../utils/fileReferences'

/**
 * The repository file open in the file viewer, kept in the `file` query
 * parameter (`?file=docs/help/serve.md:12-20`) so a reload or a shared link
 * shows it again and Back closes it.
 */
export function useFileViewer() {
    const router = hasInjectionContext() ? inject(routerKey, null) : null

    const location = computed<FileLocation | null>(() => {
        const value = router?.currentRoute.value.query.file
        return typeof value === 'string' && value.trim() ? parseFileLocation(value) : null
    })

    function target(active: Router, next: FileLocation | null) {
        const route = active.currentRoute.value
        const { file: _, ...query } = route.query
        return {
            path: route.path,
            query: next ? { ...query, file: formatFileLocation(next) } : query,
            hash: route.hash,
        }
    }

    function fileHref(next: FileLocation): string {
        if (!router) return `?${new URLSearchParams({ file: formatFileLocation(next) }).toString()}`
        return router.resolve(target(router, next)).href
    }

    /** Open a file; `replace` changes the open one without a new history entry. */
    async function openFile(next: FileLocation, options: { replace?: boolean } = {}) {
        if (!router) return
        if (options.replace) await router.replace(target(router, next))
        else await router.push(target(router, next))
    }

    async function closeFile() {
        if (router && location.value) await router.replace(target(router, null))
    }

    return { location, fileHref, openFile, closeFile }
}
