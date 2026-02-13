# D-Link

Sistema de gestion para dispositivos TI-Nspire con orquestacion asincrona de hardware.

## Caracteristicas Tecnicas

*   **Gestion Asincrona de Hardware:** Monitorizacion USB mediante callbacks de interrupcion, evitando el uso de polling y minimizando el consumo de CPU.
*   **Calculo de Directorios No Bloqueante:** Implementacion de tareas de fondo para la computacion recursiva de tamanos de carpeta con validacion de generacion para prevenir condiciones de carrera.
*   **Flujo de Edicion en Vivo:** Integracion atomica con editores externos del sistema operativo para scripts Python y Lua, con sincronizacion automatica tras la persistencia local.
*   **Inspector de Hardware Detallado:** Deteccion heuristica de entornos Ndless, versiones de OS y estados de gestion de energia.
*   **Seleccion Inteligente:** Sistema de filtrado en tiempo real y seleccion masiva basada en extensiones de archivo y metadatos.

## Requisitos del Sistema

### Dependencias de Compilacion
*   **Rust:** Toolchain estable (edition 2021).
*   **Linux:** `libusb-1.0-dev`, `pkg-config`.
*   **Windows:** Driver `WinUSB` (instalable via Zadig).
*   **macOS:** `libusb` (via Homebrew).

## Compilacion e Instalacion

### Desde codigo fuente
```bash
# Compilacion optimizada (LTO Fat + Level 3)
cargo build --release

# Los binarios resultantes se encuentran en target/release/d-link
```

## Configuracion

La aplicacion utiliza un esquema de persistencia basado en TOML ubicado en el directorio de configuracion estandar del sistema operativo:

*   **Linux:** `~/.config/d-link/config.toml`
*   **Windows:** `%AppData%\d-link\config.toml`
*   **macOS:** `~/Library/Application Support/d-link/config.toml`

## Operatividad (Keybindings)

### Navegacion
*   `UP/DOWN` | `j/k`: Desplazamiento por la tabla de archivos.
*   `ENTER` | `l`: Acceso a directorios.
*   `BACKSPACE` | `h`: Retorno al directorio superior.
*   `/`: Modo de filtrado en tiempo real.

### Operaciones de Archivo
*   `e`: Edicion en vivo (Live Edit) en editor externo.
*   `d`: Descarga de elementos seleccionados al host.
*   `n`: Creacion de nuevos directorios.
*   `r`: Renombrado de archivos o carpetas.
*   `Supr`: Eliminacion con confirmacion de integridad.

### Herramientas y Sistema
*   `i`: Inspector de propiedades detalladas.
*   `a`: Seleccion masiva de documentos (.tns).
*   `c`: Limpieza de buffer de seleccion.
*   `s`: Captura de pantalla del hardware (RGB8/L8).
*   `q` | `ESC`: Finalizacion de la sesion.

## Distribucion y Seguridad

Los binarios distribuidos incluyen un archivo `SHA256SUMS.txt` para la validacion de integridad. El flujo de CI/CD automatizado genera paquetes nativos:
*   **Linux:** `.deb` (Debian/Ubuntu) y `.rpm` (Fedora/Arch compatible).
*   **Windows:** Binario estatico optimizado.
*   **macOS:** Soporte dual para Intel y Apple Silicon.

## Licencia

GPL v3.0
