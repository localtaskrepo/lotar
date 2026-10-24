/// <reference types="vite/client" />
declare module '*.vue' {
    import type { DefineComponent } from 'vue'
    // Standard Vue SFC shim; the real prop types come from vue-tsc.
    // eslint-disable-next-line @typescript-eslint/no-empty-object-type, @typescript-eslint/no-explicit-any
    const component: DefineComponent<{}, {}, any>
    export default component
}

interface ImportMetaEnv {
    readonly VITE_APP_VERSION?: string
    readonly VITE_CARGO_VERSION?: string
}
interface ImportMeta {
    readonly env: ImportMetaEnv
}
