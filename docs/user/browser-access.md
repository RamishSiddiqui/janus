# Using Janus from a browser

Janus can serve itself over your network, so you can chat from a phone, tablet or another computer. The desktop app stays the host: your data lives on that machine, and the browser is another window onto it. Several people can share one Janus, each with their own account.

## Turn it on

1. Open **Settings, Network Access** in the desktop app. If nobody has an account yet, the page first asks you to **create the admin account** (username, a passphrase of at least 12 characters), then shows you a **recovery key** to save.
2. Switch **Browser access** on and press **Apply**. The page then shows where Janus is running, for example `http://127.0.0.1:7860`.
3. To let other devices in, also switch on **Other devices on my network** and press **Apply**. Leave it off to allow only this computer.
4. On the other device, open `http://<this computer's address>:<port>` (or `https://` if you turned on HTTPS, see below) in a browser. Find the computer's address in its network settings (it often looks like `192.168.x.x`). The device must be on the same network.

### Encrypting the connection (HTTPS)

Switch on **Encrypt with HTTPS** and press **Apply** to serve Janus over an encrypted connection. Janus makes its own certificate, so nothing leaves your network and no account or internet is needed. Because no certificate authority vouches for it, each browser shows a warning the first time (for example "Your connection is not private"). Choose **Advanced**, then **Continue**; it asks again only if Janus makes a new certificate. To be sure the warning is about your Janus, compare the fingerprint the browser shows with the one on the Network Access page.

Janus makes a new certificate when this computer's network addresses change or it is close to expiring, and the browsers on your other devices will ask again. Turn HTTPS off and the address goes back to `http://`.

### Using Tailscale

[Tailscale](https://tailscale.com/download) links your own devices in a private, encrypted network, so you can reach Janus from your phone away from home without opening your router. Install it on this computer and on the other device, sign in to the same account on both, then switch on **Browser access** and **Other devices on my network** in Janus. Under **Tailscale** on the Network Access page, Janus shows the address to open (it starts with `100.`). Traffic between your devices is already encrypted by Tailscale; Janus's own data never goes through Tailscale's servers except as encrypted relay traffic they cannot read. Tailscale needs an internet connection and an account to sign in and find your devices.

If another device cannot connect, your computer's firewall may be blocking the port. Allow Janus (or that port, on private networks only) through it.

## Signing in

The first screen is the sign-in gate. Choose **Enter**, then type your username and passphrase. **Keep this device signed in for 30 days** keeps you signed in on that device; without it the session ends after about 12 hours. **Use recovery key instead** lets you set a new passphrase if you forgot yours.

After signing in, the welcome screen lists the devices signed in to your account and lets you sign any of them out. **Step inside** opens Janus. **Sign out** (top right) ends the session.

## Accounts

- The **first account** is the **admin**.
- **Who can create accounts** (Settings, Network Access) is either **Open** (anyone who can reach Janus can sign up, from the gate) or **Admin adds** (only the admin creates accounts).
- When the admin adds someone, they give them a **temporary passphrase** of at least 12 characters. The person must choose their own when they first sign in.
- Each account gets a **recovery key**, shown once.
- **Each account has its own data**: characters, personas, chats, memories, lorebooks, providers and API keys, and generated images. Accounts cannot see each other's. The admin controls who may sign up and manages the account list. Because the admin can also act as any account from the desktop (below), the admin is trusted with everyone's data on that machine.
- Three wrong passphrases in a row from the same device or address lock that device out of the account for five minutes. Someone guessing at your account from elsewhere cannot lock you out; you can still sign in from your own device.
- The admin can delete an account; that deletes its data and files.

### Acting as another account (desktop only)

In **Settings, Network Access, Accounts**, the admin can press **Use this window as them** to use the desktop window as any account without its passphrase, for example to help someone or check what they see. A banner shows who you are acting as; **Return to admin** switches back, again without a passphrase. This is not available from a browser.

A member (not the admin) who signs in on the desktop does so with a passphrase, in **Settings, Network Access, This window**.

## What works in a browser

Almost everything: saving your data export and scene images to your device (they download like any file), chatting with streaming replies, characters and personas, memories, lorebooks, providers and models, scene images, voice with the built-in engine's streamed audio, settings, and uploads (attach an image, paste a screenshot, upload a portrait, import a character or persona card using the browser's own file picker). Layout adapts to phones: the sidebar becomes a menu button at the top left, and the chat's info panel slides over the conversation.

## Limits today

- **Desktop only:** network settings, account switching, the voice model download, and the log files (they belong to the computer running Janus).
- Settings such as theme are stored per device, so a phone and the desktop keep their own.

## Security notes

- Passphrases are stored only as argon2id hashes. Sessions use a cookie the page cannot read.
- **Without HTTPS, Janus serves plain HTTP.** Anyone who can watch traffic on your network could read what you send, including passphrases. Turn on **Encrypt with HTTPS**, or use it only on a home network you trust, never on public Wi-Fi. Either way, **do not expose the port to the internet**.
- Provider API keys are stored encrypted on the host. A browser sees only the providers of the account signed in to it, the same as the desktop app does, and a saved key is shown only as its last four characters; other accounts' providers and keys are never returned.
- Requests from other websites are rejected, so a page you visit cannot act on your Janus.
- Janus only answers requests addressed to an IP address, `localhost`, a machine name, or a local-network name (`.local`, `.lan`, a Tailscale `.ts.net` name). A request using any other domain name is refused. That stops a website from pointing its own domain at your machine to reach Janus (DNS rebinding). If you put Janus behind your own domain name, list it in the `JANUS_ALLOWED_HOSTS` environment variable (comma separated) before starting Janus.
- The page Janus serves carries a strict Content-Security-Policy: it loads only from Janus itself and cannot send data to other sites.
