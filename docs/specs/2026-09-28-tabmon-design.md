# tabmon — Diseño v1

- **Fecha:** 2026-09-28
- **Estado:** aprobado en conversación, pendiente de revisión escrita

## 1. Objetivo

CLI en Rust para usar una tablet Android como **segunda pantalla extendida** en niri, conectada por USB. Convierte en herramienta publicable un script propio que ya funciona (vkms + wayvnc + `adb reverse`).

- **Por qué:** Hyprland y Sway pueden crear salidas virtuales; niri no. El truco de `vkms` (una pantalla falsa del kernel) es poco conocido, y las alternativas (Weylus, Deskreen) van por navegador y están pensadas para duplicar, no para extender.
- **Público:** usuarios de niri en Arch → GitHub + AUR.
- **Criterio de éxito de v1:** instalando el paquete AUR y siguiendo el README, `tabmon toggle` (desde un atajo de niri) enciende y apaga la tablet como pantalla extendida; `tabmon doctor` explica qué falta cuando algo no está listo; los tests pasan en CI.
- **Fuera de v1:** Hyprland y Sway, conexión por Wi-Fi, widget o icono en la bandeja, TUI, app Android propia, `status --json`, códigos de salida por tipo de error, completions, página man, `tabmon-git` / `tabmon-bin`, publicación en crates.io.

## 2. Cómo funciona

1. El módulo del kernel **`vkms`** crea una salida falsa (`Virtual-1`).
2. En `config.kdl` el usuario le da modo, escala y posición, y la deja `off`.
3. `tabmon on` enciende la salida en niri, hace `adb reverse tcp:5900 tcp:5900` y lanza **wayvnc** en `127.0.0.1` solo sobre esa salida.
4. En la tablet, un cliente VNC conecta a `localhost:5900`, que por el `adb reverse` llega al PC por USB.

wayvnc **nunca** escucha fuera de localhost: no hay contraseña porque no hace falta, solo se llega por USB.

## 3. Comandos

```
tabmon on       enciende la pantalla virtual y arranca la conexión
tabmon off      lo para todo y deja el sistema como estaba
tabmon toggle   on/off según el estado (pensado para un atajo de niri)
tabmon status   si está activo, la salida, el puerto y el dispositivo adb
tabmon doctor   revisa vkms, salida en niri, wayvnc, adb y dispositivo; explica cómo arreglar lo que falle
```

Opción global: `-v / --verbose` imprime cada comando externo que se ejecuta.

El CLI **nunca** modifica configuraciones del sistema ni pide root.

## 4. Configuración

`~/.config/tabmon/config.toml` (respetando `XDG_CONFIG_HOME`). Opcional: si no existe, se usan los valores por defecto. Un fichero parcial rellena con los valores por defecto lo que falte.

```toml
output  = "Virtual-1"   # salida vkms definida en config.kdl
port    = 5900
max_fps = 90
notify  = true          # notificación de escritorio al encender/apagar y en errores
# adb_serial = "..."    # solo si hay varios dispositivos conectados
```

## 5. Comportamiento

### Estado

`$XDG_RUNTIME_DIR/tabmon/state` guarda lo que arrancó tabmon: PID de wayvnc, puerto y serial adb. Se borra al apagar y desaparece solo al reiniciar (vive en el runtime dir). La salida de wayvnc va a `$XDG_RUNTIME_DIR/tabmon/wayvnc.log`.

"Activo" significa: hay estado y su PID sigue siendo un wayvnc vivo (se comprueba en `/proc/<pid>/comm`).

### `on`

1. Si ya está activo → mensaje "already on", salida 0.
2. Comprueba que la salida existe en niri (`niri msg --json outputs`) y que hay un dispositivo adb utilizable (con `adb_serial` si hay varios).
3. Enciende la salida (`niri msg output <output> on`) y **espera a que esté activa** (el campo `logical` deja de ser `null`), con un tope de ~5 s.
4. `adb reverse tcp:<port> tcp:<port>`.
5. Lanza `wayvnc -r --max-fps=<max_fps> -o <output> 127.0.0.1 <port>` en **su propio grupo de procesos**, para que siga vivo cuando `tabmon` termine. Tras lanzarlo, comprueba que sigue vivo.
6. Guarda el estado y notifica (si `notify = true`).

Si cualquier paso falla, **deshace lo hecho hasta ese momento** en orden inverso (quitar el reverse, apagar la salida). Solo deshace lo que este `on` cambió: si la salida ya estaba encendida antes, no la apaga. Si un paso del rollback falla, se sigue con los demás y se informa del error original y del fallo del rollback.

### `off`

1. Si no está activo → limpia el estado que quede, mensaje "already off", salida 0.
2. Manda SIGTERM **solo al wayvnc que arrancó tabmon** (PID guardado, comprobando que sigue siendo wayvnc). Nunca `pkill`: no toca otros wayvnc.
3. Quita **solo su** reverse: `adb reverse --remove tcp:<port>` (nunca `--remove-all`).
4. Apaga la salida (`niri msg output <output> off`).
5. Borra el estado y notifica.

Si el estado está desfasado (PID muerto o reutilizado por otro proceso), no mata nada, limpia lo que pueda sin fallar.

### `toggle`

`off` si está activo, `on` si no.

### `status`

Texto legible: activo o no, salida, puerto y dispositivo adb.

### `doctor`

Una línea ✓/✗ por comprobación, con una pista para cada fallo:

- módulo `vkms` cargado
- la salida configurada existe en niri
- `niri`, `wayvnc` y `adb` están instalados
- hay un dispositivo adb autorizado (o el de `adb_serial`)

Sale con 1 si alguna comprobación falla.

## 6. Errores y mensajes

- Mensajes en **inglés** (público de AUR/GitHub). Sin traducciones en v1.
- Formato por stderr, código de salida 1:

  ```
  error: no adb device found
  hint: connect the tablet by USB and accept the debugging prompt
  ```

  Cada error dice qué ha pasado y, cuando se sabe, cómo arreglarlo. Las pistas son las mismas que da `doctor`.
- Con `notify = true`, los errores **también** se muestran con `notify-send` con urgencia crítica: desde el atajo no hay terminal y stderr no lo ve nadie.
- Programa ausente → `error: wayvnc not found` + `hint: pacman -S wayvnc` (igual para `adb` → `android-tools` y `niri`).
- La salida no llega a activarse en ~5 s → error.
- wayvnc muere al arrancar (p. ej. puerto ocupado) → error que apunta a `wayvnc.log`.
- Códigos de salida: solo 0 (bien) y 1 (error).

## 7. Estructura del código

Un solo crate binario, sin workspace:

```
src/
  main.rs          clap: parsea el comando y despacha
  config.rs        lee config.toml o usa los valores por defecto
  state.rs         lee/escribe $XDG_RUNTIME_DIR/tabmon/state
  compositor/
    mod.rs         trait Compositor (salida existe / encender / apagar / está activa)
    niri.rs        implementación con `niri msg --json`
  adb.rs           dispositivos, reverse, remove
  vnc.rs           lanzar wayvnc desacoplado y pararlo por PID
  commands/        on.rs, off.rs, toggle.rs, status.rs, doctor.rs
```

- **Crates:** `clap` (derive), `serde` + `toml`, `serde_json`, `anyhow`, `nix` (solo `signal`). Dev: `tempfile`. Versiones a confirmar al escribir el plan.
- **niri por `niri msg --json`**, no por el crate `niri-ipc`: ese crate va atado a cada versión de niri; el CLI es más estable.
- **Notificaciones** con `notify-send` como proceso externo (sin librerías D-Bus).
- **Rutas XDG** leídas de las variables de entorno (`XDG_CONFIG_HOME`, con `~/.config` por defecto; `XDG_RUNTIME_DIR`), sin el crate `dirs`.
- **Llamadas externas** (`niri`, `adb`, `wayvnc`, `notify-send`) y la comprobación de procesos pasan por un helper común, para poder simularlas en los tests.
- `trait Compositor` deja preparado añadir Hyprland/Sway en v2; en v1 solo existe `Niri`.

## 8. Tests

**Unitarios** (`cargo test`, sin tablet ni niri):

- `config`: sin fichero → valores por defecto; fichero parcial → completa; TOML inválido → error claro.
- `state`: guardar/leer; fichero ausente o corrupto; estado desfasado.
- `niri`: parseo de `niri msg --json outputs` con **fixtures reales** capturados del sistema (salida encendida, apagada, inexistente).
- `adb`: parseo de `adb devices` (ninguno, uno, varios, *unauthorized*) y selección por `adb_serial`.

**Lógica de comandos con el helper simulado:**

- `on` feliz: orden correcto de pasos y estado guardado.
- Rollback: falla `adb reverse` → se apaga la salida; wayvnc muere al arrancar → se quitan el reverse y la salida.
- Idempotencia: `on` ya activo, `off` ya apagado.
- `off` con PID viejo o reutilizado por otro proceso → no mata nada.
- Errores con su `hint` y notificación cuando `notify = true`.

**Sin tests de integración automáticos** (en CI no hay niri, vkms ni tablet): checklist manual en `docs/manual-testing.md`, a pasar con la tablet antes de cada release.

**CI (GitHub Actions):** `cargo fmt --check`, `cargo clippy -- -D warnings`, `cargo test`.

La implementación se hace con TDD.

## 9. Empaquetado y documentación

### AUR: paquete `tabmon`

- Compila desde el tarball de cada release etiquetada (`vX.Y.Z`) en GitHub.
- `depends=(niri wayvnc android-tools)`, `optdepends=('libnotify: desktop notifications')`, `makedepends=(cargo)`.
- Sigue las guías de Rust de Arch: `cargo fetch --locked`, compilación con `--frozen`, `cargo test` en `check()`.
- Instala `/usr/lib/modules-load.d/tabmon-vkms.conf` con `vkms`.
- Copia del `PKGBUILD` en el repo, en `packaging/aur/`. La subida al AUR es manual en v1.

### Licencia

MIT.

### README (inglés)

1. Qué es y por qué (niri no crea salidas virtuales; el truco de vkms).
2. Cómo funciona en tres líneas.
3. Instalación: AUR o `cargo build --release`.
4. Puesta en marcha: vkms, bloque `output "Virtual-1"` de ejemplo para `config.kdl`, depuración USB y cliente VNC en la tablet (p. ej. AVNC).
5. Atajo de ejemplo para niri (`Mod+Shift+T` → `tabmon toggle`).
6. Uso y `config.toml`.
7. Problemas → `tabmon doctor`.
8. Seguridad: solo localhost y por USB.
9. Limitaciones de v1: solo niri, solo USB.

### `docs/`

- `specs/`: este documento.
- `plans/`: plan de implementación.
- `manual-testing.md`: checklist manual de la sección 8.
