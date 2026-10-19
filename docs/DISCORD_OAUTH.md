# Discord OAuth2 en RUNE

Estado al 2026-10-14: adaptador experimental implementado y probado con
servidores HTTP locales controlados. La ventana nativa abre con una aplicación
registrada; el primer intento en navegador fue rechazado con `redirect_uri de OAuth2
no válido`. Se está corrigiendo el registro del redirect; consentimiento y
canje contra Discord todavía no están verificados. No se incorpora el Social SDK.

## Alcance

`rune-discord` solicita exactamente `identify guilds`: identidad propia y
listado básico de servidores. No habilita canales, historial, mensajes,
amigos, presencia ni llamadas. La UI muestra una página de cuenta y lista de
servidores de solo lectura; el chat completo sigue en el proveedor simulado.

La interfaz conoce `AuthenticationState` y comandos del proveedor, nunca
tokens ni protocolos de Discord. Solo el host selecciona el adaptador.

## Configurar y probar

1. Crear una aplicación en el [Developer Portal](https://discord.com/developers/applications).
2. En OAuth2 activar **Public Client** y registrar exactamente
   `http://127.0.0.1:48731/oauth/callback`. Guardar los cambios.
3. Copiar el **Application ID**, que es público. No copiar un client secret
   ni un token de cuenta o bot.
4. Desde el entorno del repositorio ejecutar:

   ```sh
   nix-shell
   cargo run -p rune-desktop -- --provider discord --discord-client-id TU_APPLICATION_ID
   ```

   También se acepta `RUNE_DISCORD_CLIENT_ID`. El modo predeterminado sigue
   siendo `--provider mock`. Discord desactiva SQLite incluso con `--data`.
5. Pulsar **Iniciar sesión**, luego **Abrir navegador**, revisar el
   consentimiento de Discord y autorizar. RUNE solo muestra **Identidad
   verificada** después de comprobar la autorización y obtener los datos.
6. Cerrar sesión descarta el estado local y solicita revocación remota. Si
   no se confirma, aparece un aviso. Cerrar el proceso o cancelar la petición
   no garantiza revocación remota: puede retirarse RUNE en Discord → Ajustes
   → Aplicaciones autorizadas.

### Si Discord muestra «redirect_uri de OAuth2 no válido»

Es un rechazo previo al consentimiento y al canje. Comparar la URL de
**OAuth2 → Redirects** después de guardar y recargar el portal; no confundir
ese campo con Application ID, Installation Link o Interactions Endpoint URL.
Debe pertenecer a la misma aplicación seleccionada al iniciar RUNE y ser
exactamente `http://127.0.0.1:48731/oauth/callback`, con puerto, `http` y sin
barra final. Después cancelar el intento anterior e iniciar uno nuevo.
No retirar PKCE ni cambiar a credenciales privadas para resolver este error.

El intento dura hasta cinco minutos. Cancelar cierra el listener y descarta
una respuesta tardía. Si el puerto está ocupado, se informa el error; no se
cambia a otro redirect. El botón de reintento requiere otra acción de la
persona. No hay refresco automático ni sesión recordada entre ejecuciones.

## Flujo implementado

1. Generar `state` y verifier independientes con 32 bytes aleatorios cada
   uno; derivar challenge SHA-256 codificado como base64url sin padding.
2. Escuchar en `127.0.0.1:48731` antes de ofrecer el enlace de autorización.
   Se usa el navegador externo con `response_type=code`, `state`,
   `code_challenge_method=S256` y los dos scopes.
3. Aceptar el callback con método, host, origen, ruta y parámetros
   validados. Si se incluye `iss`, debe ser `https://discord.com`. No se
   reflejan códigos ni estado en la respuesta HTML.
4. Canjear en `POST https://discord.com/api/oauth2/token` usando formulario
   con `client_id`, código, redirect y `code_verifier`, sin secreto.
5. Verificar `GET https://discord.com/api/v10/oauth2/@me`: aplicación
   esperada, scopes exactos, vencimiento e identidad no bot. Obtener
   `GET https://discord.com/api/v10/users/@me/guilds` con paginación acotada.
6. Publicar un snapshot de identidad/servidores y `SignedIn`. No fabricar
   canales, roles, presencia ni permisos de comunicación.

El canje público HTTP directo es una **prueba de interoperabilidad**: hay
evidencia oficial de clientes públicos y PKCE, pero no se presenta como una
integración probada hasta completar el flujo con la aplicación registrada.
Un rechazo no dispara métodos alternativos de autenticación.

## Límites y protección

Las credenciales viven en el trabajador de red, solo en RAM. No se envían
como eventos de UI ni entran en SQLite, diagnósticos o capturas. Las
respuestas HTTP tienen límite de 1 MiB; se bloquean redirecciones y se
aplican plazos de conexión/petición. El callback limita encabezados a 8 KiB,
cada petición a dos segundos y las solicitudes inválidas a 32.

Cerrar sesión, fallar o caducar limpia dominio, timelines, trabajo pendiente
y estado de UI. La sesión OAuth no restaura datos simulados. No se asegura
borrado criptográfico de memoria ni protección frente a otro proceso que
ya controle la cuenta del sistema operativo.

## Validación

- 17 pruebas del adaptador: vector PKCE, parser de callback, scopes,
  aplicación, identidad, vencimiento, paginación, límites, redirección 307 a
  un receptor de prueba, cancelación durante el canje y salida local antes
  de una revocación fallida.
- Pruebas de estado/núcleo: limpieza de cuenta y solicitudes pendientes,
  rechazo de respuestas tardías y runtime Tokio con I/O.
- Pruebas de UI: inicio, cancelación, navegación, identidad, servidores de
  solo lectura, avisos de revocación y vencimiento.
- Workspace: **335 pruebas aprobadas**, formato, `cargo check`, Clippy con
  `-D warnings` y compilación release. Cargo Audit: 0 avisos conocidos;
  detalle en [SECURITY.md](SECURITY.md).
- Prueba nativa Wayland con REA: captura de login guardada, exit 0; detalle
  y evidencias en [REA.md](REA.md).
- Validación contra Discord: **bloqueada inicialmente por redirect no
  registrado o no coincidente**, según captura aportada por el usuario.
  Pendientes consentimiento y resultado del canje. No se registran tokens ni datos de cuenta en la
  evidencia técnica. La apertura de ventana no demuestra autenticación.

## Fuentes y decisión

- [OAuth2](https://docs.discord.com/developers/topics/oauth2) y
  [User resource](https://docs.discord.com/developers/resources/user):
  endpoints y alcance de identidad/servidores.
- [Metadatos oficiales](https://discord.com/.well-known/openid-configuration):
  soporte S256; no declaran `token_endpoint_auth_methods_supported`.
- [Account Linking on Web](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-web#step-4-exchange-the-code-for-tokens):
  los clientes públicos nativos usan PKCE en lugar de secreto.
- [Account Linking with Discord](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-with-discord):
  configuración y flujo; el uso de sus documentos no incorpora su SDK.
- [Social SDK Terms §2.b(vii)](https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms):
  restricción sobre productos que compiten con Discord. No se adopta el SDK
  ni se deduce autorización para un cliente de chat por disponer de sus APIs.

La alternativa documentada de un cliente confidencial requiere un backend
propio que conserve el secreto. No se implementa ni se incrusta ese secreto
en el ejecutable. Cualquier ampliación de comunicación exige otro mecanismo
autorizado y verificado, no simplemente agregar scopes al login actual.
