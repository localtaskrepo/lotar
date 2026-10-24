// ESLint flat config for the TypeScript/Vue frontend, smoke tests and Node
// scripts (DEV-161). Formatting is Prettier's job: eslint-config-prettier
// switches off every stylistic rule that could conflict with it.
import js from '@eslint/js';
import prettier from 'eslint-config-prettier';
import { defineConfig } from 'eslint/config';
import pluginVue from 'eslint-plugin-vue';
import globals from 'globals';
import tseslint from 'typescript-eslint';

export default defineConfig(
    {
        ignores: ['**/node_modules/', 'target/', 'docs/', '.tasks/', '.history/', '.kilo/', '.playwright-mcp/'],
    },
    js.configs.recommended,
    tseslint.configs.recommended,
    pluginVue.configs['flat/recommended'],
    {
        files: ['**/*.vue'],
        languageOptions: { parserOptions: { parser: tseslint.parser } },
    },
    {
        languageOptions: {
            ecmaVersion: 'latest',
            sourceType: 'module',
            globals: { ...globals.browser, ...globals.node },
        },
    },
    {
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
            '@typescript-eslint/no-unused-vars': [
                'error',
                {
                    argsIgnorePattern: '^_',
                    varsIgnorePattern: '^_',
                    caughtErrorsIgnorePattern: '^_',
                    destructuredArrayIgnorePattern: '^_',
                },
            ],
        },
    },
    {
        // TypeScript already resolves globals and types.
        files: ['**/*.ts', '**/*.vue'],
        rules: { 'no-undef': 'off' },
    },
    {
        // Tests mock freely and define throwaway components inline.
        files: ['view/__tests__/**', 'smoke/**'],
        rules: {
            '@typescript-eslint/no-explicit-any': 'off',
            'vue/one-component-per-file': 'off',
        },
    },
    prettier,
);
