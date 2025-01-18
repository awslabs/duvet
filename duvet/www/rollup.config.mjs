import swc from '@rollup/plugin-swc';
import nodeResolve from '@rollup/plugin-node-resolve';
import replace from '@rollup/plugin-replace';
import commonjs from '@rollup/plugin-commonjs';
import typescript from '@rollup/plugin-typescript';
import postcss from 'rollup-plugin-postcss';
import terser from '@rollup/plugin-terser';
import server from 'rollup-plugin-serve';
import livereload from 'rollup-plugin-livereload';
import { basename } from 'node:path';
import { symlink, stat } from 'node:fs/promises';
const __dirname = import.meta.dirname;

const PORT = parseInt(process.env.PORT || '3000');
const NODE_ENV = process.env.NODE_ENV || 'development';
const isDev = NODE_ENV == 'development';
const isProd = NODE_ENV == 'production';

const plugins = [];

if (isDev) {
  plugins.push(
    ...[
      cjsNoParse({
        isMatch: (id) => {
          if (id.endsWith('react.development.js')) return [];
          if (id.endsWith('react-dom-client.development.js'))
            return ['react', 'react-dom', 'scheduler'];
          if (id.endsWith('react-jsx-dev-runtime.development.js'))
            return ['react'];
          if (id.endsWith('react-router')) return ['react'];
        },
      }),
      prodNoopLoad(),
      snapResolve(),
      server({
        contentBase: 'public',
        port: PORT,
        onListening(server) {
          console.log(`Server listening at http://localhost:${PORT}/`);
        },
      }),
      livereload('public'),
    ],
  );
}

plugins.push(
  ...[
    // only check types in production build
    isProd && typescript(),
    nodeResolve({
      extensions: ['.mjs', '.js', '.json', '.ts', '.tsx'],
    }),
    swc({
      exclude: ['node_modules/**', '**/main.css'],
      swc: {
        jsc: {
          parser: {
            syntax: isDev ? 'typescript' : 'ecmascript',
            tsx: true,
          },
          transform: {
            react: {
              runtime: 'automatic',
              development: isDev,
            },
          },
        },
      },
    }),
    commonjs({
      include: ['node_modules/**'],
      sourceMap: false,
    }),
    replace({
      preventAssignment: false,
      'process.env.NODE_ENV': JSON.stringify(NODE_ENV),
    }),
    postcss({
      extract: false,
      minimize: isProd,
    }),
  ],
);

if (isProd) plugins.push(terser({ mangle: true, format: { comments: false } }));

export default {
  input: isProd ? 'src/main.prod.ts' : 'src/main.dev.ts',
  output: {
    dir: 'public',
    format: isDev ? 'es' : 'iife',
  },
  plugins,
  watch: {
    exclude: 'node_modules/**',
  },
  moduleContext: isDev ? (id) => (id.endsWith('.tsx') ? 'window' : null) : null,
};

/**
 * Resolves integration snapshot files for development
 */
function snapResolve(opts = {}) {
  const frontMatter = JSON.stringify(
    import.meta.resolve('./src/util/front-matter.mjs').replace('file://', ''),
  );

  return {
    name: 'snap-resolve',

    load(id) {
      if (!id.endsWith('.snap')) return null;

      return {
        code: '<<DEV_SNAPSHOT>>',
      };
    },

    transform: async function (code, id) {
      if (!id.endsWith('.snap')) return null;

      const name = basename(id);
      const outPath = `${__dirname}/public/${name}`;

      try {
        await stat(outPath);
      } catch (_err) {
        await symlink(id, outPath);
      }

      const out = `
      import { remove as removeFrontMatter } from ${frontMatter};
      const json = fetch(${JSON.stringify(name)}).then((res) => res.text()).then((text) => JSON.parse(removeFrontMatter(text)));
      export default json;
      `;

      return {
        code: out,
        map: {},
      };
    },
  };
}

/**
 * Disables loading anything that ends with `.production.js` to avoid parsing it
 */
function prodNoopLoad() {
  return {
    name: 'prod-noop-load',

    load(id) {
      // trim any queries
      id = id.split('?')[0];

      if (!id.endsWith('.production.js')) return null;

      return {
        code: 'module.exports = {}',
      };
    },
  };
}

/**
 * Defers parsing commonjs dependencies until runtime
 *
 * This greatly speeds up dev cycle time but shouldn't be used in the prod build
 */
function cjsNoParse(opts = {}) {
  const isMatch = opts.isMatch || (() => false);

  return {
    name: 'cjsNoParse',
    enforce: 'pre',
    transform(code, id) {
      const imports = isMatch(id);
      if (!imports) return;

      const formatted = imports
        .map(
          (exp) =>
            `imports[${JSON.stringify(exp)}] = require(${JSON.stringify(exp)});`,
        )
        .join('\n');

      const encoded = JSON.stringify(
        code.replace('process.env.NODE_ENV', JSON.stringify(NODE_ENV)),
      );

      return {
        moduleSideEffects: true,
        code: `const js = ${encoded};
const init = new Function('module', 'exports', 'require', js);
const imports = {};

${formatted}

function _req(id) {
        const out = imports[id];
        if (!out) throw new Error('could not resolve module: ' + id);
        return out
}

init(module, module.exports, _req);
`,
      };
    },
  };
}
