import js from '@eslint/js';
import ts from 'typescript-eslint';
import svelte from 'eslint-plugin-svelte';
import prettier from 'eslint-config-prettier';
import globals from 'globals';

export default ts.config(
  {
    // Generated or vendored output that is not ours to lint.
    ignores: ['build/', '.svelte-kit/', 'node_modules/', 'src-tauri/', 'docs/', 'src/lib/services/bindings.ts'],
  },
  js.configs.recommended,
  ...ts.configs.recommended,
  ...svelte.configs['flat/recommended'],
  prettier,
  ...svelte.configs['flat/prettier'],
  {
    languageOptions: {
      globals: { ...globals.browser, ...globals.node },
    },
    rules: {
      // Underscore-prefixed names are deliberately unused (callback signatures, destructuring).
      '@typescript-eslint/no-unused-vars': [
        'error',
        { argsIgnorePattern: '^_', varsIgnorePattern: '^_', caughtErrorsIgnorePattern: '^_' },
      ],
    },
  },
  {
    files: ['**/*.svelte', '**/*.svelte.ts', '**/*.svelte.js'],
    languageOptions: {
      parserOptions: { parser: ts.parser },
    },
  },
  {
    files: ['**/*.ts', '**/*.svelte'],
    rules: {
      // The TypeScript compiler already reports undefined names, and this rule
      // wrongly flags DOM types such as ScrollBehavior. typescript-eslint recommends turning it off.
      'no-undef': 'off',
    },
  },
  {
    files: ['**/*.svelte'],
    rules: {
      // Guards against base-path bugs. Janus is a static app served from the site root.
      'svelte/no-navigation-without-resolve': 'off',
      // False positive for `$bindable(default)` and `$props()` defaults, which are used
      // when a parent does not pass the prop.
      'no-useless-assignment': 'off',
      // Backlog rather than blockers: adopting these everywhere changes rendering and
      // reactivity behavior, so each needs a deliberate, tested change.
      'svelte/require-each-key': 'warn',
      'svelte/prefer-svelte-reactivity': 'warn',
    },
  },
  {
    rules: {
      '@typescript-eslint/no-explicit-any': 'warn',
    },
  },
);
