# First alert in five minutes

From `docker compose up -d` to a message on your phone, without reading the rest of
the documentation. Each step says what you should see.

| Step | What you do | Typical time |
|---|---|---|
| 1. Start | `docker compose up -d` (first pull of the image) | 30 to 60 s |
| 2. Setup code | `docker compose logs dumbmonit` | 15 s |
| 3. Account | Open the page, accept the certificate once, create the admin account | 1 min |
| 4. Something to watch | Type one address, press Enter | 15 s |
| 5. Where alerts go | One click on ntfy (or Discord, Telegram, email, webhook), then **Add channel** | 30 s |
| 6. Proof | **Send a test alert**: the message arrives | 15 s |

About three and a half minutes, most of it the image download and the account.

## 1. Start the container

```bash
mkdir dumbmonit && cd dumbmonit
curl -fsSLO https://raw.githubusercontent.com/selfhostr/dumbmonit/main/docker-compose.yml
docker compose up -d
docker compose logs dumbmonit   # the one-time setup code, for example K7XQ4-M9PRT
```

## 2. Create the account

Open `https://<your-host>:8080`. The certificate is self-signed at first start: accept
the warning once ([replace it later](https.md)). Enter the setup code, a user name and
a password of at least 12 characters, then **Create account and continue**.

## 3. Your first alert

The setup wizard opens on **Your first alert**, three links of one chain. The dot of
each link turns green, with the word *done*, once it is proven.

1. **What should we watch?** Type one address and press **Watch it**:
    - `https://nas.lan` or any URL: a website check (HTTP status, certificate);
    - `192.168.1.10` or `router.lan`: a ping;
    - `nas.lan:22`: a TCP port.

    The monitor is probed at once, not at the next minute: within a few seconds you
    read *nas.lan answers.* (or why it does not). Anything else (SNMP, Proxmox, the
    agent, a NAS) is under **Another kind**, in a new tab.
2. **Where should the alert reach you?** One click on a channel kind. The form only
   asks for what that kind needs, and the name defaults to the kind.
    - **ntfy** is the quickest to a phone: a private topic is already filled in. Add
      the channel, install the ntfy app on your phone and subscribe to the topic shown,
      or open the link under it.
    - Discord takes a webhook URL, Telegram a bot token and a chat ID, email an SMTP
      server, webhook any URL. **Other channels** lists all of them.
3. **Prove it.** **Send a test alert** sends a message to that channel through the same
   notifier as a real alert. You should read *Test sent to ntfy* and see the message
   on the phone.

That is the whole road. **Continue** goes on to the name and language of the instance,
more monitors and the agent; **Skip setup** leaves the wizard at any time.

## When does a real alert arrive? {#when-does-a-real-alert-arrive}

The built-in rules are on from the start and notify every enabled channel. With the
default 60 s check interval:

| The monitor | Rule | From the outage to the message |
|---|---|---|
| Website, ping or TCP port stops answering | *Service down*: two minutes of failed checks (at least three in a row) | 2.5 to 4 min |
| Any other monitor stops reporting | *Monitor unreachable*: no measurement for three minutes, confirmed on the next cycle | 3 to 4.5 min |

A single missed check never raises an alert. Alerts that fire close together leave as
one message (30 s grouping window, *Settings → Notifications*). Every delay is a
setting: the check interval on the monitor, the delay of each rule in *Alerts → Rules*.

## Measuring it on your instance

*Settings → System → About → First run* shows when this instance passed each step:
started without an account, administrator created, first monitor, first channel, first
message delivered (a test counts), first real alert delivered, each with its delay from
the start. These times stay on the instance; nothing is sent anywhere.

## Next

- [Add more monitors](first-device.md): SNMP, a network scan, Proxmox, the agent.
- [Notification channels](../notifications.md) and [alert rules](../alerting/rules.md).
- Back up *Settings → Backup* **and** `/data/secret.key`: see [Backup](backup.md).
