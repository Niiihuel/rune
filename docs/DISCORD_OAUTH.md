# Discord OAuth2 en RUNE

`rune-discord` es el proveedor **oficial**: OAuth2 de escritorio con PKCE,
sin secreto y sin el Social SDK. Está probado con servidores HTTP locales;
**el consentimiento y el canje contra Discord no están verificados** (el
primer intento real lo rechazó Discord con «redirect_uri de OAuth2 no
válido» por el registro de la aplicación). El chat de una cuenta real va
por el proveedor no oficial ([DISCORD_SESSION.md](DISCORD_SESSION.md)).

## Qué permite la vía oficial

- Discord no ofrece un mecanismo oficial para que un cliente de terceros
  opere una cuenta personal completa. OAuth2 concede operaciones por
  scope; identidad y servidores no habilitan leer ni enviar mensajes, y
  los scopes no se convierten en permisos del Gateway
  ([OAuth2](https://docs.discord.com/developers/topics/oauth2)).
- Una cuenta bot accede a REST y Gateway con identidad de bot. El dueño
  descartó ese camino: RUNE no es un administrador de bots.
- El [Social SDK](https://docs.discord.com/developers/discord-social-sdk/overview)
  da amigos, presencia, DMs (historial limitado a 200 mensajes y 72 h) y voz
  en lobbies, orientado a juegos y experimental en Linux. Sus
  [términos §2.b(vii)](https://support-dev.discord.com/hc/en-us/articles/30225844245271-Discord-Social-SDK-Terms)
  restringen productos que compitan con Discord: RUNE no lo incorpora y no
  deduce autorización por poder llamar sus APIs.
- Reglas que siguen valiendo para cualquier proveedor: sólo acciones
  iniciadas por la persona (nada de respuestas automáticas ni actividad de
  relleno), sin archivado masivo ni uso del contenido para entrenar
  modelos, y backoff con los rate limits documentados
  ([Developer Policy](https://support-dev.discord.com/hc/en-us/articles/8563934450327-Discord-Developer-Policy)).

## Alcance

`rune-discord` pide exactamente `identify guilds`: identidad propia y
listado de servidores. La UI muestra una página de cuenta y la lista de
servidores de sólo lectura; no hay canales, historial, mensajes, amigos,
presencia ni llamadas. Ampliar la comunicación exige otro mecanismo
autorizado, no agregar scopes a este login.

La interfaz conoce `AuthenticationState` y comandos del proveedor, nunca
tokens ni protocolos de Discord. Sólo el host elige el adaptador.

## Configurar y probar

1. Crear una aplicación en el [Developer Portal](https://discord.com/developers/applications).
2. En OAuth2 activar **Public Client** y registrar exactamente
   `http://127.0.0.1:48731/oauth/callback`. Guardar los cambios.
3. Copiar el **Application ID**, que es público. No copiar un client secret
   ni un token de cuenta o bot.
4. Ejecutar:

   ```sh
   nix-shell
   cargo run -p rune-desktop -- --provider discord --discord-client-id TU_APPLICATION_ID
   ```

   También se acepta `RUNE_DISCORD_CLIENT_ID`. El modo predeterminado es
   `--provider mock`. Con Discord, SQLite queda desactivado incluso con `--data`.
5. **Iniciar sesión** → **Abrir navegador**, revisar el consentimiento y
   autorizar. RUNE sólo muestra **Identidad verificada** después de
   comprobar la autorización y obtener los datos.
6. Cerrar sesión descarta el estado local y pide la revocación remota; si no
   se confirma, aparece un aviso. Cerrar el proceso no garantiza la
   revocación: se puede retirar RUNE en Discord → Ajustes → Aplicaciones
   autorizadas.

**Si Discord muestra «redirect_uri de OAuth2 no válido»:** es un rechazo
previo al consentimiento. El redirect de **OAuth2 → Redirects** (no
Installation Link ni Interactions Endpoint URL) tiene que ser de la misma
aplicación y exactamente `http://127.0.0.1:48731/oauth/callback`, con
puerto, `http` y sin barra final. Cancelar el intento anterior y empezar
otro. No se resuelve quitando PKCE ni pasando a credenciales privadas.

El intento dura hasta cinco minutos. Cancelar cierra el listener y descarta
una respuesta tardía. Si el puerto está ocupado se informa; no se cambia a
otro redirect. Reintentar requiere otra acción de la persona. No hay
refresco automático ni sesión recordada entre ejecuciones.

## Flujo

1. `state` y verifier independientes de 32 bytes aleatorios; challenge
   SHA-256 en base64url sin padding.
2. Escuchar en `127.0.0.1:48731` antes de ofrecer el enlace. Navegador
   externo con `response_type=code`, `state`, `code_challenge_method=S256`
   y los dos scopes.
3. Callback con método, host, origen, ruta y parámetros validados; si viene
   `iss`, tiene que ser `https://discord.com`. La respuesta HTML no refleja
   código ni estado.
4. Canje en `POST https://discord.com/api/oauth2/token` con `client_id`,
   código, redirect y `code_verifier`, sin secreto.
5. Verificar `GET /api/v10/oauth2/@me` (aplicación esperada, scopes exactos,
   vencimiento, identidad no bot) y pedir `GET /api/v10/users/@me/guilds`
   con paginación acotada.
6. Publicar identidad y servidores y `SignedIn`. No se fabrican canales,
   roles, presencia ni permisos de comunicación.

Un rechazo no dispara métodos alternativos de autenticación. Un cliente
confidencial necesitaría un backend propio que guarde el secreto: no se
implementa ni se incrusta un secreto en el ejecutable.

## Límites y protección

Las credenciales viven en el trabajador de red, sólo en RAM: no viajan en
eventos de UI ni entran en SQLite, diagnósticos o capturas. Las respuestas
HTTP tienen límite de 1 MiB, se bloquean redirecciones y hay plazos de
conexión y de petición. El callback limita encabezados a 8 KiB, cada
petición a dos segundos y las solicitudes inválidas a 32.

Cerrar sesión, fallar o caducar limpia dominio, timelines, trabajo pendiente
y estado de UI; la sesión OAuth no restaura datos simulados. No se asegura
borrado criptográfico de memoria ni protección frente a otro proceso que ya
controle la cuenta del sistema.

## Fuentes

- [OAuth2](https://docs.discord.com/developers/topics/oauth2) y
  [User resource](https://docs.discord.com/developers/resources/user).
- [Metadatos OpenID](https://discord.com/.well-known/openid-configuration):
  soporte S256.
- [Account Linking on Web](https://docs.discord.com/developers/discord-social-sdk/development-guides/account-linking-on-web#step-4-exchange-the-code-for-tokens):
  los clientes públicos nativos usan PKCE en lugar de secreto.
