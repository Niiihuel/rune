# Desarrollo paralelo por funcionalidades

Coordinación solicitada por el dueño el 2026-10-14. Hay tres trabajadores
simultáneos y un integrador. No se confunden trabajo asignado, código
implementado, pruebas locales y funcionamiento contra un servicio real.

## Continuidad desde ambos clones de Serein (2026-10-14)

Se revisaron `serein` (`d1af883c`) y `serein-current` (`5f83077e`) en modo
de sólo lectura; ambos checkouts quedaron limpios. Inventario de faltantes,
evidencia y orden de trabajo en [SEREIN_COMPARISON.md](SEREIN_COMPARISON.md#inventario-para-la-siguiente-entrega-paralela).
La especificación de esta entrega y su lista completada están en
[tasks/plan.md](../tasks/plan.md) y [tasks/todo.md](../tasks/todo.md).

- `audit_auth_invites`: entrada de enlace desde «+», reconocimiento local
  por el adaptador y preview separado; creación de servidores con permiso
  revalidado. Seis regresiones nuevas y 26 pruebas de invitaciones aprobadas.
- `audit_ui`: filtros locales «Solo míos», «Con archivos» y «Fijados»;
  cuatro regresiones nuevas y 24 pruebas de búsqueda aprobadas.
- `audit_provider_media`: marcadores optativos de lectura con alcance local
  explícito, cuatro regresiones; presencia de privados sin estado inventado,
  una regresión adicional. El integrador conectó la lectura automática.
- Integrador: inventario, capturas, revisión de contratos, formato y checks
  globales. La revisión cruzada corrigió acciones antiguas que quitaban foco
  a una invitación o descartaban un formulario de servidor ya abierto.

Con `nix-shell --arg withLogin true --arg withDocs true` pasaron
`cargo fmt --all -- --check`, `cargo test --workspace --all-features`
(**651 aprobadas, seis ignoradas, 23 suites**), Clippy de todos los targets
y features con `-D warnings`, y release con `session-login`. Se añadieron
15 pruebas respecto de la base de 636. La mutación del guard de lectura
hizo fallar ambas regresiones de alcance local; se restauró antes de integrar.

El release abrió y terminó con código 0 en las dos capturas revisadas:
[búsqueda con archivos](img/rune-search-filters.png) y
[entrada de invitación](img/rune-invitation-input.png). El harness aplica el
fixture de invitación antes de las acciones para reproducir el formulario
sin consultas externas. Las pruebas y capturas no usaron cuentas reales;
la aceptación experimental permanece pausada. No cambió Cargo.lock ni se
añadieron dependencias, migraciones o APIs de limn.
[Evidencia y hashes](evidence/native-workflows-2026-10-14.json).

Las secciones siguientes conservan el historial de entregas anteriores.

## Estado de las entregas

| Frente | Responsable | Trabajo actual | Validación requerida |
| --- | --- | --- | --- |
| Login, invitación, scroll | Integrador | Bienvenida propia, modal compacto, aceptación por sesión experimental y barra nativa de limn | Aceptación implementada; validación y límites en [INVITATIONS.md](INVITATIONS.md), unión real pendiente |
| Perfiles e imágenes | `login_reference` | Avatar coherente, página adaptable, banners recibidos y combinación de datos parciales | Modelo/adapters/UI/decoder probados; REA sobre geometría; edición remota pendiente |
| GIFs/stickers | `expressions_audit` | Primer cuadro, preview de sticker y buscador GIPHY con configuración propia | HTTP local, cancelación, paginación y atribución; 8 tests ejecutados por REA; falta clave y prueba real |
| PDF/archivos | `invite_flow` | Vista nativa explícita de la primera página con Poppler aislado | PDF propio, límites, cancelación y proceso real observado por REA; Linux/Nix opcional |
| Servidores, roles y permisos | Integrador + revisión cruzada | Revalidación al guardar/confirmar y durante reordenación; auditoría respeta revocaciones | 25 pruebas ejecutadas por REA; acciones remotas siguen dependiendo de capacidades verificadas |

## Contrato de cada trabajador

1. Identificar el recorrido completo: dato, operación, estado y componente.
2. Leer Serein como referencia, sin copiar código ni activos. Usar fuentes
   oficiales para distinguir interfaces documentadas de rutas no oficiales.
3. Acordar archivos propios y contratos compartidos antes de editar.
4. Implementar un incremento funcional, sin habilitar capacidades ficticias.
5. Ejecutar pruebas locales representativas. Usar REA sobre un artefacto
   construido o escenario controlado que responda una pregunta concreta;
   leer un repositorio no se etiqueta como ingeniería inversa.
6. Entregar evidencia, límites y pendientes al integrador.

El código y la investigación avanzan en paralelo. Cargo y las capturas de
ventanas se coordinan para evitar contención y confundir qué ventana se
observa. Las capturas de procesos sin UI pueden ser independientes. Nadie
cierra ni cambia el objetivo compartido de REA mientras otro lo usa.
Los fixtures no consultan cuentas reales, envían mensajes ni aceptan
invitaciones automáticamente.

## Integración

El integrador revisa los cambios, ejecuta fmt/check/test/clippy, construye el
release y revisa las capturas pertinentes. Mantiene limn como interfaz y
preserva cambios anteriores del workspace. Si falta una API de limn, el
cambio se desarrolla en su worktree con pruebas y PR; no se reemplaza el
framework. Una integración que necesita una clave propia se configura por
separado; las pruebas sintéticas no acreditan conectividad real.

La aceptación de invitaciones tiene ahora operación y pruebas propias;
su validación contra Discord sigue pendiente. La edición remota de perfiles
sigue separada de sus mejoras visuales. No se considera terminada una
operación por haber construido su botón o su formulario.

## Resultado de integración

Entorno `nix-shell --arg withLogin true --arg withDocs true`:

- `cargo fmt --all -- --check`: correcto.
- `cargo check --workspace --all-features`: correcto.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: correcto.
- `cargo test --workspace --all-features`: **562 aprobadas**, seis ignoradas,
  23 suites. El test opcional de sandbox PDF se ejecutó aparte y pasó.
- `cargo build --release -p rune-desktop --features session-login`: correcto.
  El ejecutable final abrió en Wayland con fixtures locales y salió con código
  0; [captura revisada](img/rune-attachments.png).

Evidencia por frente: [PDF](DOCUMENTS.md), [perfiles](PROFILES.md),
[GIFs](EXPRESSIONS.md), [administración](evidence/rea-administration-2026-10-14.json)
y [capturas e investigación](REA.md). REA observó binarios propios y fixtures;
estas pruebas no acreditan operaciones sobre cuentas ni envíos reales.

La revisión cruzada corrigió: cancelación de GIF en el mismo cuadro de cierre,
fallback ante miniaturas lentas, textos alternativos vacíos/largos y recargas
de auditoría tras perder permiso. No se copió código de Serein.

## Próximos incrementos

1. Voz: escucha local de prueba, evaluación auditiva con muestras autorizadas,
   medición de latencia y referencia de reproducción para cancelar eco.
2. Validar GIPHY con una aplicación propia; mantener separado el envío de chat.
3. Reproducción animada con presupuesto de cuadros y pausa fuera del viewport.
4. Navegación multipágina PDF y empaquetado del aislamiento fuera de Nix.
5. Carga ampliada y edición remota de perfiles y administración
   sólo cuando haya contratos y autorización verificables para cada operación.

## Entrega de audio local (2026-10-14)

Trabajaron en paralelo un agente en captura y ciclo de vida, otro en DSP y
mediciones y otro en controles limn. El integrador conectó la captura única
al host, revisó los cambios y ejecutó los controles globales. Serein se
estudió como código público; no se copió su implementación.

Se añadió reducción moderada de ruido Sonora, prueba con niveles de entrada
y procesada, captura fuera de UI y cola acotada. Los cambios de dispositivo,
mute y stop descartan generaciones anteriores. Las revisiones corrigieron
un despertar perdido tras una apertura lenta, silencio con umbral cero,
indicadores de voz retenidos tras fallos, recuperación del worker y etiquetas
de accesibilidad de los medidores. La enumeración de dispositivos se agrupa
y un hilo fallido permite volver a intentarlo.

Validación con el mismo entorno Nix: fmt/check/Clippy correctos y
`cargo test --workspace --all-features`: **590 aprobadas, seis ignoradas,
23 suites**. Las pruebas nuevas y sus replays REA usan señales sintéticas y
backends falsos; no abren micrófonos ni transmiten audio.

REA ejecutó [23 pruebas de audio](evidence/rea-audio-tests-2026-10-14.json),
[18 de ajustes](evidence/rea-audio-ui-2026-10-14.json) y el
[benchmark release](evidence/rea-audio-processing-2026-10-14.json).
El p95 a 48 kHz fue 37 µs por bloque de 10 ms. Eso mide procesamiento
sintético, no calidad perceptual ni latencia de conversación; ver
[MEDIA.md](MEDIA.md) y [PERFORMANCE.md](PERFORMANCE.md).

El release con `session-login` compiló y se ejecutó en Wayland, salida 0.
Se revisó la [pantalla de voz](img/rune-audio.png) con datos simulados y el
control de ruido activado, sin iniciar la prueba del micrófono. Su
[registro REA](evidence/rea-audio-window-2026-10-14.json) corresponde al
ejecutable SHA-256
`3fdabdf0ad8c69e5f11d4d793396944911e8014581267bd2d8c6fe0861b75909`.
La ventana de la cuenta que ya estaba abierta se conservó; utiliza el
ejecutable anterior hasta reiniciar.

## Entrega de invitaciones (2026-10-14)

Tres agentes trabajaron en contrato/capacidades y revisión, UI limn y
adaptador de sesión. El integrador añadió plazos y control de cola en core,
identificadores monotónicos entre sesiones y el fixture gráfico. Serein se
leyó como fuente; no se copió código ni se ejecutó su cliente.

El modal ahora permite aceptar desde la sesión experimental y espera la
membresía/canales reales para navegar. Consulta, aceptación y navegación
tienen estados separados; una respuesta tardía no reabre el modal ni repite
la escritura. Los detalles no oficiales y las verificaciones no soportadas
se registran en [INVITATIONS.md](INVITATIONS.md).

Validación final: fmt/check/Clippy correctos; **627 pruebas aprobadas, seis
ignoradas, 23 suites**. Release compilado y ventana Wayland propia revisada
mediante REA, salida 0. [Captura](img/rune-invitation.png),
[pruebas del adaptador](evidence/rea-invitations-2026-10-14.json) y
[registro de ventana](evidence/rea-invitation-window-2026-10-14.json).
Durante esa validación no se cerró ni utilizó la sesión abierta del dueño.

## Pausa de la aceptación de invitaciones (2026-10-14)

Tres agentes revisaron en paralelo Serein, Gateway y el flujo de invitaciones;
el integrador ejecutó Cargo. Se cerró la instancia experimental y se pausó la
aceptación tanto en capacidades como en el proveedor. Los cierres terminales
conservan ahora su código numérico sin el motivo remoto; 4004 elimina la
sesión y los trabajos pendientes.

Validación: fmt/check/Clippy correctos, **631 pruebas aprobadas, seis ignoradas,
23 suites**. Una mutación local de la condición 4004 hizo fallar su regresión;
se restauró antes de la suite final.

## Adaptación del ciclo de sesión de Serein (2026-10-14)

Se clonó `references/serein-current` en `5f83077`, conservando la referencia
anterior y ambas fuera del control de versiones de RUNE. Se implementó un
clasificador REST propio compartido entre acciones e invitaciones; distingue
credencial inválida, cuenta por verificar y verificación de una operación.
El proveedor descarta la sesión y cola sólo cuando corresponde; no reproduce
la identificación de otro cliente ni reenvía desafíos.

Un agente implementó transporte y fixtures, otro regresiones de sesión y
otro revisó los cambios. El integrador conectó `ends_session`, documentó
las diferencias, verificó la sensibilidad de una regresión y ejecutó Cargo.
**636 pruebas aprobadas**, seis ignoradas; fmt/check/Clippy correctos.
La aceptación real sigue pausada. [Mecanismo y fuentes](SEREIN_SESSION_FLOW.md).
