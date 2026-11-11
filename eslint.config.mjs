// ESLint checks only the templates of the Vue components (eslint-plugin-vue): oxlint has no template rules. oxlint
// (.oxlintrc.json) checks all TypeScript and JavaScript, including the <script> blocks. Formatting is Prettier's
// job: eslint-config-prettier switches off every stylistic rule that could conflict with it.
import prettier from 'eslint-config-prettier';
import { defineConfig } from 'eslint/config';
import pluginVue from 'eslint-plugin-vue';
import tseslint from 'typescript-eslint';

export default defineConfig(
    {
        // Only .vue files: everything else is oxlint's.
        ignores: [
            '**/node_modules/',
            'target/',
            'docs/',
            '.tasks/',
            '.history/',
            '.kilo/',
            '.playwright-mcp/',
            '**/*.{js,cjs,mjs,jsx,ts,cts,mts,tsx}',
        ],
    },
    {
        files: ['**/*.vue'],
        extends: [pluginVue.configs['flat/recommended']],
        // The script blocks are TypeScript; the parser only has to read them.
        languageOptions: { parserOptions: { parser: tseslint.parser } },
        linterOptions: { reportUnusedDisableDirectives: 'error' },
        rules: {
            // Style the codebase deliberately does not follow; formatting
            // and ordering are not correctness concerns.
            'vue/attributes-order': 'off',
            'vue/attribute-hyphenation': 'off',
            'vue/v-on-event-hyphenation': 'off',
            'vue/multi-word-component-names': 'off',
            // Optional TypeScript props are undefined by design.
            'vue/require-default-prop': 'off',
            // The task panel shares one reactive form object with its
            // sections; editing its fields is intended, replacing it is not.
            'vue/no-mutating-props': ['error', { shallowOnly: true }],
        },
    },
    {
        // Tests define throwaway components inline.
        files: ['view/__tests__/**'],
        rules: { 'vue/one-component-per-file': 'off' },
    },
    prettier,
);
