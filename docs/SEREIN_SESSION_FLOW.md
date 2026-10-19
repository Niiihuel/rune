# Adaptación del ciclo de sesión de Serein

## Referencia y alcance

El 2026-10-14 se clonó una referencia nueva, independiente y de solo lectura:
`references/serein-current`, commit
[`5f83077e33e10971159ec7dd56b5c7f21a9902b8`](https://github.com/ViceVerse-cz/Serein/tree/5f83077e33e10971159ec7dd56b5c7f21a9902b8).
Se conserva `references/serein` en `d1af883` para reproducir las revisiones
anteriores. Ambos quedan excluidos del control de versiones de RUNE.
La referencia declara `MIT OR Apache-2.0` y contiene ambas licencias.
No se ejecutó Serein ni se incorporó su código o dependencias al producto.

Se adapta con código propio la separación entre transporte, resultado de
una acción y ciclo de sesión. La interfaz sigue construida con limn y no
recibe códigos de API, desafíos ni credenciales.

Esto **no reproduce el mecanismo completo de ingreso de Serein**: no añade
su identificación de navegador, procesamiento/reenvío de CAPTCHA ni otras
continuaciones de autenticación. Tampoco reactiva la aceptación de
invitaciones, en pausa entonces.

## Patrón estudiado

- El transporte clasifica rechazos y dispone de `stop()` para impedir nuevas
  solicitudes: [discord-api](https://github.com/ViceVerse-cz/Serein/blob/5f83077e33e10971159ec7dd56b5c7f21a9902b8/crates/discord-api/src/lib.rs#L261).
- `Failure::ends_session` expresa cuándo termina la conexión:
  [client-core/auth.rs](https://github.com/ViceVerse-cz/Serein/blob/5f83077e33e10971159ec7dd56b5c7f21a9902b8/crates/client-core/src/auth.rs#L62).
- La capa de conexión detiene tareas y entrega el fallo al host:
  [connection.rs](https://github.com/ViceVerse-cz/Serein/blob/5f83077e33e10971159ec7dd56b5c7f21a9902b8/apps/desktop/src/connection.rs#L90).
- La clasificación separa desafíos de una acción y fallos de autenticación:
  [discord-api](https://github.com/ViceVerse-cz/Serein/blob/5f83077e33e10971159ec7dd56b5c7f21a9902b8/crates/discord-api/src/lib.rs#L457).

RUNE ya serializa las operaciones REST dentro del proveedor. Cuando termina
la sesión, descarta Gateway, receptor, cola y credencial en RAM; publica
`AuthenticationState::Failed`, que limpia el dominio y la UI. No necesita
copiar el worker ni añadir otro runtime de Serein.

## Clasificación de RUNE

| Respuesta | Tratamiento |
| --- | --- |
| HTTP 401 autenticado | Cerrar sesión sin esperar el cuerpo. |
| HTTP 400/403, código 50014 | Credencial inválida; cerrar la sesión autenticada. |
| HTTP 400/403, código 40002 o 50178 | Cuenta requiere verificación; detener la sesión autenticada, sin resolver ni repetir automáticamente. |
| HTTP 400/403, código 60003 | Verificación de esa operación; rechazarla conservando la sesión. |
| HTTP 400/403 con campos de desafío no nulos | Verificación de esa acción; no procesar ni reenviar el desafío. |
| Consulta anónima con los mismos códigos | No invalidar la sesión de la cuenta. |
| HTTP 429 | Conservar cooldown compartido; ninguna repetición automática de escritura. |
| Respuesta incierta después de una escritura | Resultado ambiguo; no afirmar éxito ni reenviar. |

La semántica de los códigos procede de la
[tabla oficial de Discord](https://docs.discord.com/developers/topics/opcodes-and-status-codes#json).
En particular, Serein agrupa `60003` y `50014` como desafíos terminales;
RUNE los distingue porque MFA para una operación no equivale a una credencial
inválida. Los códigos de cuenta tienen prioridad sobre la presencia de campos
de CAPTCHA. No se diagnostica suspensión o intrusión a partir de estas categorías.

`http/rejections.rs` concentra el clasificador. HTTP general y la ruta de
invitaciones comparten sus resultados, con lectura de rechazos limitada a
64 KiB y el timeout de transporte. El mensaje remoto y el contenido de
desafíos se ignoran; al proveedor sólo salen etiquetas locales y la decisión
`ends_session`. No se añadieron dependencias, persistencia ni cambios visuales.

## Verificación

Las pruebas usan servidores HTTP/WebSocket locales, identidades sintéticas
y login falso. Cubren códigos, prioridad, datos malformados, límites,
ausencia de material remoto en errores, cierre con trabajo en cola y
verificación de acción sin repetición de solicitudes. La política de
aceptación en producción continúa cerrada incluso ante órdenes forzadas.

Validación en `nix-shell --arg withLogin true --arg withDocs true`:

- `cargo fmt --all -- --check`: correcto.
- `cargo check --workspace --all-features`: correcto.
- `cargo test --workspace --all-features`: **636 aprobadas**, seis ignoradas,
  23 suites. El adaptador tiene 115 pruebas aprobadas, incluidos doctests.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`:
  sin incidencias.
- `cargo build --release -p rune-desktop --features session-login`: correcto.
  SHA-256 de `target/release/rune`:
  `90d3b86237e76c966532e414bef0c4d1ff6f8d6bc72fbf3ce45318f2df295f93`.
  Se compiló sin abrir una sesión real.

La revisión independiente encontró que Serde puede deserializar un struct
desde un array posicional. Se exige raíz de objeto para impedir que un array
con un número coincidente se convierta en rechazo de sesión. Al retirar
temporalmente esa guarda, la regresión falló como se esperaba; se restauró
antes de la suite completa. La revisión no dejó defectos pendientes en este
alcance. No se conectó el adaptador a Discord ni se usó la cuenta afectada.
