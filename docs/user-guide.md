# Hashlark user guide

Hashlark searches many torrent indexers at once. It downloads what you pick with its built-in torrent client, or hands it to your own torrent client if you turn the built-in one off. It hosts no content and sends no telemetry.

## Install

- **Windows:** run `Hashlark_x.y.z_x64-setup.exe` (installs for your user only) or the `.msi`.
- **macOS:** open the `.dmg` and drag Hashlark to Applications.
- **Linux:** use the `.AppImage` (make it executable), or install the `.deb`.
- **Server / NAS:** see [deploy/README.md](deploy/README.md).

Hashlark has its own torrent client, on by default, so you don't need another one. If you turn it off (see [Downloading](#downloading)), you need a torrent client that opens magnet links, for example qBittorrent, Transmission or Deluge. Hashlark tells you if none is set up.

## Searching

1. Type what you're looking for and press **Enter**. Press **/** anywhere to jump to the search box.
2. Optionally pick **categories** (Movies, TV, Software, …). Only providers with those categories are searched.
3. Results appear as each provider answers:
   - the chips above the list show each provider's status, result count and speed;
   - the same torrent found on several sites is shown once, listing every source.
4. Sort by best match, seeders, size or date.

On each result:

| Button | What it does |
|---|---|
| **Download** | Starts the torrent in Hashlark's built-in client, in your download folder, with every file. Shift+click to choose the files and folder first. When the built-in client is off, this button is **Open in client** instead. |
| **Open in client** | Sends the magnet (or `.torrent`) to your own torrent client. Shown beside Download when the built-in client is on. |
| **Copy magnet** | Copies the magnet link. |
| **Send to device** | Sends the magnet to Hashlark on your phone (see [Send results to another device](#send-results-to-another-device)). |
| **.torrent** | Saves the `.torrent` file to your Downloads folder (or the folder in Settings) and opens it. |
| **★** | Saves the result to **Favourites**. |

Click a row for details: infohash, publish date, every site that has it, and a link to the site's page.

## Downloading

Hashlark's built-in torrent client downloads and seeds torrents on your computer, on the headless server and on Android. It's on by default. To use your own torrent client instead, turn off **Use the built-in torrent client** under **Settings → Torrent client**. Results then open in your own client, as they did before.

Torrent traffic connects directly. It never goes through Tor or the search proxy. The optional SOCKS5 proxy for torrents covers only TCP connections to peers and trackers. DHT and uTP (UDP) still connect directly. The Downloads page says so as well.

### Adding a torrent

- **Download** on a search result starts it at once, in your download folder, with every file.
- **Shift+click** Download opens the Add dialog for that one torrent, where you choose the files and the folder. On Android, long-press **Download** on a result's details, or pick **Download with options…** from a result's menu; there you choose the folder and whether to start paused, and you pick files later in the torrent's details.
- To see the Add dialog for every torrent, turn on **Ask where to save each torrent** in Settings.
- On the **Downloads** page in the sidebar, paste a magnet link and click **Add**. Or click **Open .torrent** and pick one or more `.torrent` files. You can also drop a `.torrent` file onto the window.
- Magnet links from your browser or other apps open in Hashlark when it is the magnet handler (see [Opening magnet links](#opening-magnet-links-with-hashlark)). Hashlark also adds `.torrent` files that you open with it.
- **Watched folder** (desktop and server): set a folder under **Settings → Torrent client → Watched folder**. Each `.torrent` file you put in it is added, then renamed to `name.torrent.added` so it isn't added twice. The folder is checked every 5 seconds.
- **One-time notice:** before your first download, Hashlark asks you to accept a notice. Downloading shares pieces with other people, and it shows them your IP address. Only download what you have the right to. The answer is remembered, and the notice isn't shown again.

### Pause, Stop and Force start

- **Pause** is temporary. The torrent keeps its place in the queue. **Resume** carries on.
- **Stop** takes the torrent out of the queue. A stopped torrent never starts on its own, and a finished one stops seeding. **Start** puts it back in the queue.
- **Force start** runs the torrent now, even when the queue is full. It doesn't skip the schedule: a schedule hour set to Off still pauses it.
- **Pause all** and **Resume all** are on the Downloads page and in the tray menu.
- On the Downloads page, Space pauses or resumes, Shift+S stops or starts, and Shift+F force starts.

### Deleting

Click **Delete** on a torrent. The dialog asks whether to delete the downloaded files too. **Also delete downloaded files** is unticked by default. Without it, the torrent and its saved state are removed, and your files stay where they are.

A torrent you created with **Create** never deletes your original files, even when the box is ticked.

### Filters and the queue

- The filters are **All**, **Downloading**, **Seeding**, **Queued**, **Paused**, **Stopped**, **Finished** and **Error**. Each shows a count. **Filter by name** narrows the list by name.
- The **Queued** filter shows the queue in order. Drag rows to reorder them. You can also select torrents and use **Move to top**, **Move up**, **Move down** or **Move to bottom**. On the keyboard, Ctrl+↑ and Ctrl+↓ move the focused row.
- By default, 3 downloads and 5 seeds run at once. Change **Active downloads** and **Active seeds** under **Settings → Torrent client → Queue and seeding**.

### Seeding

Seeding means uploading a torrent after you have it, so others can download it too. Hashlark stops seeding when a limit is reached.

- **Stop at ratio:** by default, seeding stops when you have uploaded as much as you downloaded (a ratio of 1.0) on desktop and server. On Android the default is 0, so seeding stops as soon as the download finishes.
- **Stop after (minutes):** an optional time limit. It counts wall-clock time from the moment the download finished. That includes time when the torrent was paused, and time when Hashlark was closed.
- You can change both limits for all torrents, or for one torrent.

### Connections

- **Peers:** up to 1000 connections in total and 200 per torrent on desktop and server. On Android the defaults are 400 and 100. Change them under **Settings → Torrent client → Connections**, or for one torrent. The total is shared between running torrents, so it is approximate, and incoming connections can briefly go over it.
- **Listen port:** `0` picks a random port (from 49152 to 65534) on first start, and keeps it afterwards. The headless server always uses 6881. Forward that port on your router, or let UPnP do it, so more peers can reach you.

### Speed limits and the schedule

- **Speed limits:** set download and upload limits in KiB/s. Empty means unlimited. The alternative limits, 1024 KiB/s down and 256 KiB/s up by default, are used by the turtle toggle, by schedule hours set to Alternative speed, and on battery if you choose that.
- **Turtle toggle:** the turtle icon in the sidebar turns alternative speed on or off straight away.
- **Weekly schedule:** turn on **Use a weekly schedule** to set each hour of the week. Hours are in your local time. Each hour is one of these:
  - **Normal:** the normal limits.
  - **Alternative speed:** the alternative limits.
  - **Seed only:** nothing downloads, but seeding goes on.
  - **Off:** everything is paused, including force-started torrents.
  - Click or drag to paint the grid. With the keyboard, the arrow keys move and 1 to 4 set Normal, Alternative speed, Seed only or Off.

### Power

- **Keep the computer awake:** choose **Off**, **While downloading** (the default), or **While downloading or seeding**. On Android, the same setting keeps the phone awake while it downloads.
- **When all done** (desktop only): in the Downloads toolbar, or in the tray menu, choose what happens after your last download: **Do nothing**, **Quit Hashlark**, **Sleep**, **Hibernate** or **Shut down**. A 60-second countdown then starts, and **Cancel** stops it.
  - "All done" means nothing is downloading or queued to download. Seeding doesn't count. A paused download that isn't finished counts as not done.
  - The choice goes back to **Do nothing** after it fires, and when Hashlark restarts. A shutdown can't surprise you days later.
  - Hibernate needs hibernation turned on in Windows (`powercfg /h`). It isn't offered on macOS.
  - Linux and macOS may ask for permission the first time. If an action fails, Hashlark shows a message and does nothing.
- **On battery:** on a laptop running on battery, and on Android when battery saver is on or the battery is at 15% or less and not charging, choose **Do nothing**, **Alternative speed** (the default) or **Pause all**.

### Opening magnet links with Hashlark

- **Windows and Linux:** turn on **Open magnet links with Hashlark** under **Settings → Torrent client**. Your browser and other apps then send magnet links to Hashlark. On Windows, if you have set another app for magnet links under **Default apps**, Windows keeps using that app. Change it there.
- **macOS:** choose Hashlark as the app for magnet links yourself, in Finder or in the settings of another torrent app.

### The tray

With **Keep running in the tray** on (the default), closing the window doesn't stop your downloads. Hashlark stays in the system tray instead, and a one-time message explains where it went. The tray menu has **Show**, **Pause all** or **Resume all**, **Alternative speed**, **When all done** and **Quit**. Use **Quit** to close Hashlark.

### Creating torrents

On desktop and on the server, click **Create** on the Downloads page. Choose a file or a folder to share, give it a name if you like, and add trackers, one per line, if you want them. Hashlark makes the `.torrent` and starts seeding it straight away. On the server, the file or folder must be inside one of the server's allowed folders. Android can't create torrents.

### On Android

- **Wi-Fi only by default:** torrents download only on Wi-Fi or another unmetered network. Turn on **Download on mobile data** to allow metered networks too. While a torrent waits for a suitable network, its status says **Waiting for Wi-Fi**.
- **Background downloads:** downloads run in a foreground service, with a notification that shows progress. The notification has **Pause all** and **Stop** buttons.
- **Where the files go:** `Download/Hashlark`. On Android 8 and 9, Hashlark asks for storage permission first. If it can't write to that folder, for example on some Android 10 phones, it uses the app's own Downloads folder instead. Files in the app's folder are removed when you uninstall Hashlark.
- **Battery savers:** some phone makers' battery savers stop background services, and that stops downloads. Turn off battery optimisation for Hashlark in your phone's battery settings.

### Known gaps

The built-in client doesn't do these yet:

- **Streaming:** you can't watch a video while it downloads.
- **RSS:** there are no automatic downloads from feeds.
- **Categories and tags:** there are only the status filters, plus a name filter on desktop and in the web UI.
- **Moving storage:** you can't move a torrent's files after adding it.
- **Per-file priorities:** you can select or skip files, but you can't set how important each file is.
- **Protocol encryption (MSE/PE):** not supported. Some internet providers slow down unencrypted torrent traffic, and some private trackers require encryption. For those, use another client.
- **Tracker status:** you see the tracker list, but not whether each tracker answered.
- **Performance presets.**
- **A command-line client.**

## Send results to another device

If you search on your phone but your torrent client runs on your computer, Hashlark can send a result across your Wi-Fi, and the other way round too. The other device opens it in its own torrent client. Both devices need Hashlark open and on the same network. Nothing goes through the internet.

**Phone → computer**

1. On the computer, go to **Settings → Devices on this network** and turn on **Share with devices on this network**. If Windows asks whether to allow Hashlark through the firewall, allow it. If your Wi-Fi is marked as a *Public* network, allow it there too.
2. On the phone, open a result and tap **Send to computer**, or long-press a result. Pick your computer.
3. The first time, the phone asks for a PIN. On the computer, click **Pair a phone** and type the 6-digit PIN it shows into the phone. The PIN works for 2 minutes.

From then on, **Send to computer** and a tap on the computer is all it takes, and the magnet opens on the computer.

**Computer → phone**

1. On the phone, go to **Settings → Devices on this network** and tap **Show pairing PIN**.
2. On the computer, open a result, click **Send to device** and pick the phone. Type the PIN the phone shows.

Pairing works both ways, so once a phone and computer are paired, either can send to the other without a PIN. The phone receives links only while Hashlark is open on it.

**If the other device isn't listed**

Some networks, such as guest Wi-Fi and some routers, block the way devices find each other. Choose **Enter address…** and type the address the other device shows under **Settings → Devices on this network**, for example `192.168.1.20:47821`.

**Good to know**

- Only paired devices can send to you, and links are encrypted on the network.
- To stop a device from sending, forget it under **Settings → Devices on this network**. It has to be paired again before it can send.
- Only results with a magnet link can be sent; `.torrent`-only results can't yet.

## Providers

**Providers** lists every indexer, with its health:

- *Healthy*, *Unreliable* or *Failing*, plus its success rate and typical response time;
- *Paused*: the provider failed five times in a row. It's skipped for 15 minutes, then 1 hour, then 6 hours, and retried automatically. Turning it off and on again un-pauses it straight away.

Use **Test** to try a provider now. **Details** shows its settings, network route and definition.

Hashlark starts with legal sources only: the Internet Archive, Linuxtracker, Academic Torrents, FOSS Torrents and Public Domain Torrents (classic films). To add more, use **Add provider**:

- **Definition file:** paste or load a `.yml` definition, check it, preview a search, then add it. Writing definitions is covered in [definition-format.md](definition-format.md).
- **Jackett / Prowlarr:** enter the Torznab URL and API key to search every indexer configured there, including private trackers.
- **Definition repositories** (at the bottom of the page): add a repository URL to get a curated set of definitions that update themselves daily.
  - Repositories are signed. Hashlark remembers the key and refuses updates signed with a different one.
  - New providers from a repository start **disabled**, so you can choose which to use.

Providers that need an account show their settings under **Details**. Passwords and API keys are kept in your system keychain.

## When a site blocks you

A provider's status explains what went wrong:

- **Blocked:** the site couldn't be reached. This often means your network or country blocks it. Try, in order:
  1. **Settings → Network & privacy → Encrypted DNS.** This is on by default, and gets past most DNS-based blocks.
  2. A **proxy** (HTTP or SOCKS5).
  3. **Tor**: install and start Tor (the Tor service, Tor Browser or Orbot), then turn on **Route everything through Tor**, or route just one provider through Tor under **Details → Network**. Tor is slower but reaches most blocked sites, and definitions can list `.onion` mirrors.
- **Needs browser check:** the site shows a "checking your browser" page. Click **Open site to continue**, complete the check in the window that opens, then click **Done**. Hashlark keeps that site's cookies for this provider until they expire. (Headless servers can't do this; use Jackett/Prowlarr with FlareSolverr for such sites.)
- **Site changed:** the site's layout changed and its definition needs an update. Repository definitions usually get fixed within a day.
- **Login failed:** check the provider's username, password or API key.

## Favourites and history

- **Favourites** keeps results you starred, and they still open later.
- **History** lists recent searches; click one to run it again. You can turn history off, or clear it, in **Settings**.

## Settings

| Section | What's there |
|---|---|
| Search | Timeout per provider, how many providers run at once, result caching, history. |
| Network & privacy | Encrypted DNS provider, proxy, Tor, requests per second per site. |
| Magnet links | Add public trackers to every magnet, and a tracker-list URL that's refreshed daily. |
| Downloads | Folder for `.torrent` files. |
| Torrent client | The built-in client on or off, download folder, listen port, connections, speed limits, weekly schedule, seeding, queue, power, SOCKS5 proxy for torrents, watched folder. See [Downloading](#downloading). |
| Devices on this network | Sharing results with your phone: on or off, **Pair a phone**, paired devices. |
| Appearance & updates | Light, dark or system theme; automatic update checks. |

## Android

Download the APK for your phone from the [releases page](https://github.com/ashxtrem/hashlark/releases): `arm64-v8a` fits nearly every phone (including the Galaxy Z Fold7); the `universal` APK works everywhere and is larger. Open it and allow your browser or file manager to install apps. Check the APK against `SHA256SUMS` and the signing-certificate fingerprint in the release notes if you like. Hashlark needs Android 8.0 or newer and a torrent client that opens magnet links.

Everything above works the same on Android. What is different:

- **Layouts follow the window.** On a phone or a foldable's cover screen you get one screen at a time (bottom bar; tap a result to see its details, Back returns to the list). On a tablet or an unfolded foldable the results and details sit side by side, and on very wide windows a filter panel joins them and the results become a sortable table. Half-open a foldable and Hashlark uses both halves: with the fold **horizontal** (tabletop), results are on top and the search box, chips and keyboard below; with the fold **vertical** (book), the list is on one side and the details on the other. Folding, unfolding or rotating in the middle of a search keeps the results.
- **Provider status is one chip** next to the result count ("4 providers", or "3 of 4 providers" in red when some failed) so the results get the room. Tap it for each provider's count and speed, and for the way forward when a site needs a browser check.
- **Search from anywhere:** share text or an IMDb link to Hashlark, or select text in any app and choose *Search in Hashlark*. Long-press the app icon for *New search*, *Favourites*, *History* and your last searches.
- **Send to computer:** on a result's details or its long-press menu, send it to Hashlark on your computer; see [Send results to another device](#send-results-to-another-device). **Settings, Devices on this network** turns this on or off, shows the phone's address and a pairing PIN, and lists paired devices.
- **Torrent client:** the built-in client downloads magnets and `.torrent` files. Android-specific details are in [Downloading](#on-android). With the built-in client off, magnets open in the client Android picks; choose one in **Settings, Magnets and downloads**. `.torrent` files are saved to your Downloads folder and can be opened from the message that appears.
- **Adding providers:** open a `.yml` definition from a file manager, or a `hashlark://repo?url=...` link. Hashlark always shows what it found and asks before adding. For a repository it shows the signing key first; only accept keys of people you trust.
- **Browser checks** open in a window inside the app: pass the check, then tap **Done**.
- **Tor:** built-in Tor works on Android and starts only when needed; on networks that block Tor, install Orbot (which supports bridges), start it, and choose *Orbot or my own Tor* in Settings.
- **Background sync:** repositories and the tracker list refresh about once a day, only on Wi-Fi and when the battery is not low.
- **Keyboard and mouse** (tablets, DeX, Chromebooks): `Ctrl+F` focuses the search box, arrow keys move through results, `Enter` opens, `Ctrl+C` copies the selected magnet, right-click opens the result menu. In split screen you can drag a result into a torrent client.
- **Privacy:** nothing is backed up, and passwords and API keys are encrypted with a key stored in the Android Keystore.

## Privacy

Hashlark sends no analytics or crash reports. It only connects to:

- the providers you enabled;
- definition repositories you added;
- your tracker-list URL, if you set one;
- GitHub, to check for updates (you can turn this off);
- Hashlark on your own devices on the same network, if you share results with them. They find each other with mDNS, and only paired devices can send links, which are encrypted;
- the peers, trackers and DHT of the torrents you download or seed with the built-in client.

Peers see your IP address. Torrent traffic never goes through Tor or the search proxy, and trackers and DHT use your system's DNS, not encrypted DNS. The optional SOCKS5 proxy for torrents covers TCP peers and trackers only. Turn off the built-in client if you don't want this.

Your searches, favourites, settings and torrent state stay in a database on your computer.

Hashlark is a neutral search and download tool. You are responsible for complying with the law and copyright where you live.
