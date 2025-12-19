# Migration from Create React App to Vite

This project has been migrated from Create React App (react-scripts) to Vite for better performance and React 19 support.

## What Changed

### 1. Build Tool
- **Before**: `react-scripts` (webpack-based)
- **After**: `vite` (esbuild-based, much faster)

### 2. Scripts
- **Before**: `npm start`, `npm run build`, `npm test`
- **After**: 
  - `npm run dev` or `npm start` - Start development server
  - `npm run build` - Build for production
  - `npm run preview` - Preview production build
  - `npm test` - Run tests with Vitest

### 3. Environment Variables
- **Before**: `VITE_*` prefix (e.g., `VITE_CALLISTO_BACKEND`)
- **After**: `VITE_*` prefix (e.g., `VITE_CALLISTO_BACKEND`)
- **Access**: Changed from `process.env.VITE_*` to `import.meta.env.VITE_*`

### 4. HTML Entry Point
- **Before**: `public/index.html` with `%PUBLIC_URL%` placeholders
- **After**: `index.html` in root directory with direct paths

### 5. Configuration
- **Before**: Hidden webpack config in react-scripts
- **After**: `vite.config.ts` - fully customizable

## Migration Steps Completed

1. ✅ Created `vite.config.ts`
2. ✅ Moved and updated `index.html` to root
3. ✅ Updated `package.json` scripts and dependencies
4. ✅ Removed `react-scripts` dependency
5. ✅ Added Vite and related dependencies
6. ✅ Updated all `process.env.VITE_*` to `import.meta.env.VITE_*`
7. ✅ Created `src/vite-env.d.ts` for TypeScript support
8. ✅ Updated Dockerfile environment variables

## Next Steps

### 1. Install Dependencies
```bash
cd fe/callisto
rm -rf node_modules package-lock.json
npm install
```

### 2. Update Environment Variables
If you have a `.env` file, rename variables from `VITE_*` to `VITE_*`:
```bash
# Before
VITE_CALLISTO_BACKEND=http://localhost:30000
VITE_NODE_SERVER=http://localhost:3000
VITE_GOOGLE_OAUTH_CLIENT_ID=your-id

# After
VITE_CALLISTO_BACKEND=http://localhost:30000
VITE_NODE_SERVER=http://localhost:3000
VITE_GOOGLE_OAUTH_CLIENT_ID=your-id
```

See `.env.example` for reference.

### 3. Start Development Server
```bash
npm run dev
# or
npm start
```

### 4. Build for Production
```bash
npm run build
```

The build output will be in the `build/` directory (same as before).

### 5. Preview Production Build
```bash
npm run preview
```

## Benefits of Vite

- ⚡ **Much faster** - Cold start in milliseconds vs seconds
- 🔥 **Hot Module Replacement (HMR)** - Instant updates without full reload
- 📦 **Smaller bundles** - Better tree-shaking and code splitting
- 🎯 **React 19 support** - Full compatibility with latest React
- 🛠️ **Better DX** - Clearer error messages, faster builds
- 🔧 **Customizable** - Easy to configure and extend

## Troubleshooting

### Port Already in Use
Vite uses port 3000 by default. If it's in use, it will automatically try the next available port.

### Environment Variables Not Working
- Make sure they start with `VITE_` prefix
- Restart the dev server after changing `.env` files
- Check `src/vite-env.d.ts` for TypeScript definitions

### Build Errors
- Clear node_modules and reinstall: `rm -rf node_modules package-lock.json && npm install`
- Check for any remaining `process.env.VITE_*` references
- Ensure all imports use correct paths (Vite is stricter about extensions)

### Tests Not Running
- Vitest is configured as the test runner
- Update test files if they relied on CRA-specific features
- Run `npm test` to start Vitest in watch mode

## Additional Resources

- [Vite Documentation](https://vitejs.dev/)
- [Vite React Plugin](https://github.com/vitejs/vite-plugin-react)
- [Vitest Documentation](https://vitest.dev/)
- [Migration Guide](https://vitejs.dev/guide/migration.html)

