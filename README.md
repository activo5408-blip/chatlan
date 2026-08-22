# ChatLAN (Tauri + React)

Migración completa de **ChatLAN** (antes Electron + bandeja del sistema) a
**Tauri 2 + React + TypeScript**. Misma funcionalidad:

- Chat P2P por UDP broadcast en la red local (sin servidor).
- Envío de archivos y fotos por TCP, con barra de progreso.
- Pegar imágenes del portapapeles (Ctrl+V) y arrastrar/soltar archivos.
- Ventana de "respuesta rápida" al recibir un mensaje nuevo.
- Doble check (✓ / ✓✓), indicador de "escribiendo…", historial persistido.
- Ícono en la bandeja del sistema, inicio automático con Windows.

El resultado ocupa muchísimo menos RAM/disco que la versión Electron porque
usa el WebView2 del sistema (Windows) en vez de empaquetar Chromium entero.

## 1. Requisitos previos

Tenés dos caminos para compilar la app — elegí el que prefieras:

- **Opción A: no instalar nada en tu PC** → solo necesitás una cuenta de
  GitHub (gratis). Anda directo a la sección **"2. Compilar SIN instalar
  nada"** de abajo.
- **Opción B: compilar en tu propia PC** → necesitás instalar, en este
  orden:
  1. **Node.js** (18 o más nuevo) → https://nodejs.org
  2. **Rust** → https://www.rust-lang.org/tools/install
     (en Windows, el instalador te va a pedir instalar también "C++ build
     tools" de Visual Studio — aceptalo, hace falta para compilar).
  3. **WebView2** (en Windows 10/11 ya viene instalado de fábrica; si tu PC
     es más vieja, Tauri te va a avisar y te da el link).
  4. **Requisitos de sistema de Tauri** (por si falta algo específico de tu
     SO): https://v2.tauri.app/start/prerequisites/

  Verificá que quedó todo instalado:

  ```bash
  node -v
  cargo -v
  ```

## 2. Compilar SIN instalar nada en tu PC (recomendado si no querés instalar Rust/Visual Studio)

Este proyecto ya incluye `.github/workflows/build.yml`, listo para que
**GitHub Actions** (gratis) compile la app por vos en la nube. Solo
necesitás una cuenta de GitHub — no instalás Rust, ni Node, ni Visual
Studio en tu computadora.

1. Andá a https://github.com y creá una cuenta si no tenés (es gratis).
2. Arriba a la derecha, click en el **"+"** → **"New repository"**.
   - Ponele un nombre, por ejemplo `chatlan`.
   - Dejalo en **Public** o **Private**, como prefieras.
   - Click en **"Create repository"** (no hace falta tildar nada más).
3. En la página del repo recién creado, buscá el link que dice
   **"uploading an existing file"** (o andá a la pestaña **Add file →
   Upload files**).
4. Arrastrá **todo el contenido** de la carpeta `chatlan-tauri` (todos los
   archivos y carpetas descomprimidos del zip) a esa página y confirmá
   el commit ("Commit changes").
5. Andá a la pestaña **"Actions"** del repositorio.
   - Si te pregunta si querés habilitar Actions, decí que sí.
   - Deberías ver un workflow llamado **"Compilar ChatLAN (Windows)"**
     corriendo solo (se dispara automáticamente al subir los archivos).
     Si no arrancó solo, hacé click en él y despues en **"Run workflow"**.
6. Esperá unos 5-10 minutos a que el círculo se ponga verde ✅.
7. Click en esa ejecución (el nombre del commit) → abajo del todo, en
   **"Artifacts"**, vas a ver **"ChatLAN-Windows"** → hacé click para
   descargar un `.zip` con el `.exe` ya compilado, listo para usar.

Cada vez que quieras recompilar (por ejemplo si te paso una corrección de
código), repetís el paso 4 subiendo los archivos nuevos, y Actions vuelve a
compilar solo.

## 3. Compilar en tu PC (necesita instalar Rust + Visual Studio)

Si estás en Windows, lo más simple es doble clic en **`compilar.bat`**
(está en la raíz del proyecto). El script:

1. Revisa que tengas Node.js y Rust instalados (si falta alguno, te dice
   exactamente de dónde bajarlo y se detiene).
2. Corre `npm install`.
3. Compila la app entera (`npm run tauri build`).
4. Copia el resultado final a una carpeta **`Compilado\`** con:
   - `ChatLAN.exe` → versión portable, la copiás a cualquier PC y listo.
   - el instalador `...-setup.exe`, si se generó.

Es normal que la primera vez tarde varios minutos (Cargo compila todas las
dependencias de Rust desde cero). Las próximas veces es mucho más rápido.

Si preferís hacerlo a mano, o estás en Linux/macOS, seguí los pasos
manuales de las secciones siguientes.

## 3.1. Instalar las dependencias del proyecto (manual)

Descomprimí el ZIP y, desde la carpeta del proyecto:

```bash
npm install
```

La primera vez que corras algo con Tauri (`npm run tauri dev` o
`npm run tauri build`), Cargo va a descargar y compilar automáticamente
todas las dependencias de Rust (`tauri`, `tokio`, `image`, etc.). Puede
tardar varios minutos la primera vez; las siguientes son mucho más rápidas.

## 3.2. Probar la app en modo desarrollo

```bash
npm run tauri dev
```

Esto levanta el frontend (Vite) y compila+abre la app de escritorio. Podés
dejarla abierta y editar el código: el frontend se recarga solo; si tocás
código Rust (`src-tauri/src/*.rs`) va a recompilar y reabrir la app.

Para probar el chat P2P de verdad necesitás correrlo en **dos PCs distintas
en la misma red** (o dos usuarios de Windows con la app abierta a la vez),
igual que con la versión Electron.

## 3.3. Compilar el ejecutable final

```bash
npm run tauri build
```

Al terminar vas a encontrar, dentro de `src-tauri/target/release/`:

- **`chatlan.exe`** → el ejecutable suelto (parecido al "portable" que
  generaba `electron-builder`). Podés copiarlo a cualquier PC con Windows
  10/11 y correrlo directo, sin instalar nada más.
- **`bundle/nsis/ChatLAN_2.0.0_x64-setup.exe`** → un instalador con ícono en
  el menú de inicio, desinstalador, etc. (útil si lo vas a repartir a gente
  no técnica).

En Linux se genera un `.deb`/`.AppImage`, y en macOS un `.app`/`.dmg`, si
alguna vez lo compilás en esos sistemas — el código ya es multiplataforma.

## 4. Estructura del proyecto

```
chatlan-tauri/
├── compilar.bat               # Instala y compila todo con un doble clic (Windows)
├── src/                      # Frontend (React + TypeScript)
│   ├── components/
│   │   ├── Login.tsx         # Pantalla para elegir tu nombre
│   │   ├── Panel.tsx         # Lista de contactos en la red
│   │   ├── Chat.tsx          # Ventana de conversación
│   │   └── QuickReply.tsx    # Ventanita de respuesta rápida
│   ├── lib/api.ts            # Puente hacia los comandos de Rust
│   ├── App.tsx                # Decide qué pantalla mostrar según la ventana
│   └── styles.css             # Tu mismo diseño visual de siempre
├── src-tauri/                # Backend (Rust)
│   ├── src/
│   │   ├── main.rs           # Arranque, bandeja, ciclo de vida
│   │   ├── state.rs          # Estado compartido + historial en disco
│   │   ├── net.rs            # Descubrimiento UDP + envío de archivos TCP
│   │   ├── windows.rs        # Ventanas flotantes y bandeja
│   │   └── commands.rs       # Comandos que llama el frontend
│   ├── icons/                 # Ícono de la app (ya generado)
│   └── tauri.conf.json        # Configuración de la app/ventanas
└── package.json
```

## 5. Notas de la migración (por si algo falla al compilar)

Escribí y revisé todo el código a mano, pero como este entorno no tiene
Rust/Cargo instalado no pude compilarlo acá para verificarlo. Es un proyecto
grande, así que si `npm run tauri build` tira algún error de compilación,
son casi siempre ajustes menores de nombres de método en las librerías
(los plugins de Tauri cambian pequeños detalles de API entre versiones).
Los puntos más probables:

- **Iconos**: ya generé `32x32.png`, `128x128.png`, `128x128@2x.png` y
  `icon.png`/`icon.ico` a partir de tu ícono original. Si Tauri pide más
  variantes, corré `npx tauri icon src-tauri/icons/icon-large.png`.
- **Plugins de diálogo/portapapeles/autostart**: si algún método (`.dialog()`,
  `.clipboard()`, `.autolaunch()`) no compila tal cual, revisá la doc de cada
  plugin — la API pública se mantiene muy estable entre versiones 2.x, pero
  puede haber cambiado algún nombre reciente:
  - https://v2.tauri.app/plugin/dialog/
  - https://v2.tauri.app/plugin/clipboard/
  - https://github.com/ahkohd/tauri-plugin-autostart (o el oficial de Tauri)
- Si Cargo se queja de versiones, corré `cargo update` dentro de
  `src-tauri/` para que resuelva las últimas compatibles.

Cualquier error de compilación que te tire la terminal, pegámelo en el chat
y te lo arreglo.
