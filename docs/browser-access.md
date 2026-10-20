# Using Janus from a browser

Janus can serve itself over your network, so you can chat from a phone, tablet or another computer. The desktop app stays the host: your data lives on that machine, and the browser is just another window onto it.

## Turn it on

1. Open **Settings, Network Access** in the desktop app (you must be signed in as the admin).
2. Switch **Browser access** on and press **Apply**.
3. For other devices, also switch on **Other devices on my network**. Leave it off to allow only this computer.
4. Open the address shown (for example `http://192.168.1.24:7860`) in a browser. For other devices, use this computer's address on your network.

## Accounts

- The first account created becomes the **admin**. If nobody has an account yet, the browser offers "Create the admin account".
- Each account has its own characters, personas, chats, memories and providers. Accounts cannot see each other's data. The admin only controls who may sign up.
- **Who can create accounts** is a setting: *Open* lets anyone who can reach Janus sign up; *Admin adds* means only you create accounts, with a temporary passphrase the person replaces on first sign in.
- Each account gets a **recovery key** when it is created. It is shown once. It lets the person set a new passphrase if they forget theirs.
- The desktop window acts as the admin by default. **Settings, Network Access, This window** switches it to another account.
- Deleting an account deletes its data and files.

## Security notes

- Passphrases are stored only as argon2id hashes. Sessions use a cookie the page cannot read, and repeated wrong passphrases lock an account for a while.
- Janus serves plain HTTP. Anyone who can watch traffic on your network could read what you send, including passphrases. Use it only on a home network you trust, never on public Wi-Fi, and do not expose the port to the internet.
- API keys for AI providers stay on the host. Browsers never receive them.

## Limits today

- Saving files (backups, logs, exported scene images) is not available in the browser yet.
- Network settings and the voice model downloads are managed from the desktop app.
