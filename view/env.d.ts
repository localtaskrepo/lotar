/// <reference types="vite/client" />
declare module '*.vue' {
    import type { DefineComponent } from 'vue'
    // Vue SFC shim for tools without SFC support (oxlint's type-aware rules); vue-tsc uses the real component types.
    // unknown bindings, not any, so the linter does not take `wrapper.vm as any` in tests for a no-op.
    const component: DefineComponent<Record<string, unknown>, Record<string, unknown>, unknown>
    export default component
}

interface ImportMetaEnv {
    readonly VITE_APP_VERSION?: string
    readonly VITE_CARGO_VERSION?: string
}
interface ImportMeta {
    readonly env: ImportMetaEnv
}
