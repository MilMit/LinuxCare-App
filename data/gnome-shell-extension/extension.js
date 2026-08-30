import GObject from 'gi://GObject';
import St from 'gi://St';
import GLib from 'gi://GLib';
import Gio from 'gi://Gio';
import Clutter from 'gi://Clutter';
import * as Main from 'resource:///org/gnome/shell/ui/main.js';
import * as PanelMenu from 'resource:///org/gnome/shell/ui/panelMenu.js';
import * as PopupMenu from 'resource:///org/gnome/shell/ui/popupMenu.js';
import { Extension } from 'resource:///org/gnome/shell/extensions/extension.js';

export default class LinuxCareVitalsExtension extends Extension {
    enable() {
        this._indicator = new PanelMenu.Button(0.0, this.metadata.name, false);
        this._box = new St.BoxLayout({ style_class: 'linuxcare-panel-box', reactive: true });

        // Labels for metrics
        this._tempLabel = new St.Label({ text: 'TMP —°C', y_align: Clutter.ActorAlign.CENTER, style_class: 'linuxcare-metric' });
        this._cpuLabel = new St.Label({ text: 'CPU —%', y_align: Clutter.ActorAlign.CENTER, style_class: 'linuxcare-metric' });
        this._ramLabel = new St.Label({ text: 'RAM —%', y_align: Clutter.ActorAlign.CENTER, style_class: 'linuxcare-metric' });
        this._netLabel = new St.Label({ text: 'NET —', y_align: Clutter.ActorAlign.CENTER, style_class: 'linuxcare-metric' });
        this._batLabel = new St.Label({ text: 'BAT —%', y_align: Clutter.ActorAlign.CENTER, style_class: 'linuxcare-metric' });

        this._box.add_child(this._tempLabel);
        this._box.add_child(this._cpuLabel);
        this._box.add_child(this._ramLabel);
        this._box.add_child(this._netLabel);
        this._box.add_child(this._batLabel);

        this._indicator.add_child(this._box);
        Main.panel.addToStatusArea(this.uuid, this._indicator, 1, 'right');

        // State trackers
        this._prevCpu = null;
        this._prevNet = null;
        this._config = {
            show_cpu: true,
            show_temp: true,
            show_ram: true,
            show_net: true,
            show_battery: true,
            interval_sec: 2,
        };

        this._loadConfig();
        this._applyConfigVisibility();
        this._buildMenu();

        // Initial update & configured recurring loop
        this._updateMetrics();
        this._startTimer();

        // Config file monitor
        this._setupConfigMonitor();
    }

    disable() {
        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = null;
        }
        if (this._fileMonitor) {
            this._fileMonitor.cancel();
            this._fileMonitor = null;
        }
        if (this._indicator) {
            this._indicator.destroy();
            this._indicator = null;
        }
    }


    _startTimer() {
        if (this._timeoutId) {
            GLib.Source.remove(this._timeoutId);
            this._timeoutId = null;
        }
        const intervalMs = Math.max(1, Number(this._config.interval_sec) || 2) * 1000;
        this._timeoutId = GLib.timeout_add(GLib.PRIORITY_DEFAULT, intervalMs, () => {
            this._updateMetrics();
            return GLib.SOURCE_CONTINUE;
        });
    }

    _loadConfig() {
        try {
            const path = GLib.build_filenamev([GLib.get_user_config_dir(), 'linuxcare', 'config.json']);
            const file = Gio.File.new_for_path(path);
            if (file.query_exists(null)) {
                const [success, contents] = file.load_contents(null);
                if (success) {
                    const json = JSON.parse(new TextDecoder().decode(contents));
                    if (json && json.top_bar) {
                        this._config = Object.assign(this._config, json.top_bar);
                    }
                }
            }
        } catch (e) {
            // ignore
        }
    }

    _saveConfig() {
        try {
            const dirPath = GLib.build_filenamev([GLib.get_user_config_dir(), 'linuxcare']);
            GLib.mkdir_with_parents(dirPath, 0o700);
            const path = GLib.build_filenamev([dirPath, 'config.json']);
            const file = Gio.File.new_for_path(path);

            let json = {};
            if (file.query_exists(null)) {
                const [success, contents] = file.load_contents(null);
                if (success) {
                    try {
                        json = JSON.parse(new TextDecoder().decode(contents)) || {};
                    } catch (e) {}
                }
            }
            json.top_bar = this._config;
            const data = JSON.stringify(json, null, 2);
            file.replace_contents(data, null, false, Gio.FileCreateFlags.NONE, null);
        } catch (e) {
            // ignore
        }
    }

    _applyConfigVisibility() {
        this._cpuLabel.visible = !!this._config.show_cpu;
        this._tempLabel.visible = !!this._config.show_temp;
        this._ramLabel.visible = !!this._config.show_ram;
        this._netLabel.visible = !!this._config.show_net;
        this._batLabel.visible = !!this._config.show_battery;
    }

    _buildMenu() {
        // Title Item
        const titleItem = new PopupMenu.PopupMenuItem('LinuxCare System Vitals', { reactive: false });
        titleItem.label.style_class = 'linuxcare-popup-header';
        this._indicator.menu.addMenuItem(titleItem);

        this._menuStatCpu = new PopupMenu.PopupMenuItem('CPU: —', { reactive: false });
        this._menuStatRam = new PopupMenu.PopupMenuItem('RAM: —', { reactive: false });
        this._menuStatTemp = new PopupMenu.PopupMenuItem('Temperature: —', { reactive: false });
        this._menuStatNet = new PopupMenu.PopupMenuItem('Network: —', { reactive: false });
        this._menuStatBat = new PopupMenu.PopupMenuItem('Battery: —', { reactive: false });

        this._indicator.menu.addMenuItem(this._menuStatCpu);
        this._indicator.menu.addMenuItem(this._menuStatRam);
        this._indicator.menu.addMenuItem(this._menuStatTemp);
        this._indicator.menu.addMenuItem(this._menuStatNet);
        this._indicator.menu.addMenuItem(this._menuStatBat);

        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());

        // Power Profiles Submenu
        const powerSub = new PopupMenu.PopupSubMenuMenuItem('Power Profile');
        const pPerf = new PopupMenu.PopupMenuItem('Performance');
        const pBal = new PopupMenu.PopupMenuItem('Balanced');
        const pSave = new PopupMenu.PopupMenuItem('Power Saver');

        const setProfile = (prof) => {
            if (!['performance', 'balanced', 'power-saver'].includes(prof))
                return;
            try {
                const parameters = new GLib.Variant('(ssv)', [
                    'net.hadess.PowerProfiles',
                    'ActiveProfile',
                    new GLib.Variant('s', prof),
                ]);
                Gio.DBus.system.call(
                    'net.hadess.PowerProfiles',
                    '/net/hadess/PowerProfiles',
                    'org.freedesktop.DBus.Properties',
                    'Set',
                    parameters,
                    null,
                    Gio.DBusCallFlags.NONE,
                    5000,
                    null,
                    null,
                );
            } catch (e) {}
        };

        pPerf.connect('activate', () => setProfile('performance'));
        pBal.connect('activate', () => setProfile('balanced'));
        pSave.connect('activate', () => setProfile('power-saver'));

        powerSub.menu.addMenuItem(pPerf);
        powerSub.menu.addMenuItem(pBal);
        powerSub.menu.addMenuItem(pSave);
        this._indicator.menu.addMenuItem(powerSub);

        // Quick Actions intentionally stay unprivileged in the shell extension.

        const openAppItem = new PopupMenu.PopupMenuItem('Open LinuxCare');
        openAppItem.connect('activate', () => {
            try {
                Gio.AppInfo.create_from_commandline('/usr/bin/linuxcare', null, Gio.AppInfoCreateFlags.NONE).launch([], null);
            } catch (e) {}
        });
        this._indicator.menu.addMenuItem(openAppItem);

        // Customize Visible Items Submenu
        const customizeSub = new PopupMenu.PopupSubMenuMenuItem('Customize Top Bar Display');

        const addToggle = (label, key, widget) => {
            const toggle = new PopupMenu.PopupSwitchMenuItem(label, !!this._config[key]);
            toggle.connect('toggled', (item, state) => {
                this._config[key] = state;
                widget.visible = state;
                this._saveConfig();
            });
            customizeSub.menu.addMenuItem(toggle);
        };

        addToggle('Show CPU Usage (%)', 'show_cpu', this._cpuLabel);
        addToggle('Show Temperature (°C)', 'show_temp', this._tempLabel);
        addToggle('Show RAM Usage (%)', 'show_ram', this._ramLabel);
        addToggle('Show Network Speed', 'show_net', this._netLabel);
        addToggle('Show Battery (%)', 'show_battery', this._batLabel);

        this._indicator.menu.addMenuItem(customizeSub);

        this._indicator.menu.addMenuItem(new PopupMenu.PopupSeparatorMenuItem());
        const authorItem = new PopupMenu.PopupMenuItem('Developed by MilMit (milmit.net)');
        authorItem.connect('activate', () => {
            try {
                Gio.AppInfo.launch_default_for_uri('https://milmit.net', null);
            } catch (e) {}
        });
        this._indicator.menu.addMenuItem(authorItem);
    }

    _updateMetrics() {
        // 1. CPU
        try {
            const statText = GLib.file_get_contents('/proc/stat')[1];
            const str = new TextDecoder().decode(statText);
            const line = str.split('\n').find(l => l.startsWith('cpu '));
            if (line) {
                const parts = line.trim().split(/\s+/).slice(1).map(Number);
                const user = parts[0] || 0, nice = parts[1] || 0, sys = parts[2] || 0, idle = parts[3] || 0;
                const iowait = parts[4] || 0, irq = parts[5] || 0, softirq = parts[6] || 0, steal = parts[7] || 0;
                const totalIdle = idle + iowait;
                const totalTime = user + nice + sys + totalIdle + irq + softirq + steal;

                if (this._prevCpu) {
                    const totalDiff = totalTime - this._prevCpu.total;
                    const idleDiff = totalIdle - this._prevCpu.idle;
                    if (totalDiff > 0) {
                        const pct = Math.round(((totalDiff - idleDiff) / totalDiff) * 100);
                        this._cpuLabel.text = `CPU ${pct}%`;
                        this._menuStatCpu.label.text = `CPU Usage: ${pct}%`;
                    }
                }
                this._prevCpu = { total: totalTime, idle: totalIdle };
            }
        } catch (e) {}

        // 2. Temperature (Smart Intel Coretemp / AMD K10temp priority)
        let bestTemp = null;
        let bestPrio = 0;
        try {
            for (let i = 0; i < 16; i++) {
                const namePath = `/sys/class/hwmon/hwmon${i}/name`;
                if (!GLib.file_test(namePath, GLib.FileTest.EXISTS)) continue;
                const name = new TextDecoder().decode(GLib.file_get_contents(namePath)[1]).trim().toLowerCase();
                let basePrio = 30;
                if (['coretemp', 'k10temp', 'zenpower'].includes(name)) basePrio = 90;
                else if (['cpu_thermal', 'soc_thermal'].includes(name)) basePrio = 85;
                else if (['dell_smm', 'thinkpad', 'applesmc'].includes(name)) basePrio = 70;
                else if (name === 'acpitz') basePrio = 10;
                else if (['nvme', 'iwlwifi', 'amdgpu', 'nouveau'].includes(name)) basePrio = 5;

                for (let t = 1; t <= 8; t++) {
                    const tempPath = `/sys/class/hwmon/hwmon${i}/temp${t}_input`;
                    if (!GLib.file_test(tempPath, GLib.FileTest.EXISTS)) continue;

                    let prio = basePrio;
                    const labelPath = `/sys/class/hwmon/hwmon${i}/temp${t}_label`;
                    if (GLib.file_test(labelPath, GLib.FileTest.EXISTS)) {
                        const label = new TextDecoder().decode(GLib.file_get_contents(labelPath)[1]).trim().toLowerCase();
                        if (label.includes('package') || label.includes('tctl') || label.includes('tdie')) {
                            prio += 10;
                        } else if (label.includes('core')) {
                            prio += 5;
                        }
                    }

                    const milli = parseFloat(new TextDecoder().decode(GLib.file_get_contents(tempPath)[1]).trim());
                    if (milli >= 15000 && milli <= 125000) {
                        const deg = Math.round(milli / 1000);
                        if (prio > bestPrio || (prio === bestPrio && deg > (bestTemp || 0))) {
                            bestPrio = prio;
                            bestTemp = deg;
                        }
                    }
                }
            }

            if (bestPrio < 70) {
                for (let z = 0; z < 16; z++) {
                    const typePath = `/sys/class/thermal/thermal_zone${z}/type`;
                    const tempPath = `/sys/class/thermal/thermal_zone${z}/temp`;
                    if (GLib.file_test(tempPath, GLib.FileTest.EXISTS)) {
                        let prio = 20;
                        if (GLib.file_test(typePath, GLib.FileTest.EXISTS)) {
                            const type = new TextDecoder().decode(GLib.file_get_contents(typePath)[1]).trim().toLowerCase();
                            if (['x86_pkg_temp', 'cpu-thermal', 'k10temp', 'coretemp'].includes(type)) prio = 95;
                            else if (type === 'acpitz') prio = 10;
                        }
                        const milli = parseFloat(new TextDecoder().decode(GLib.file_get_contents(tempPath)[1]).trim());
                        if (milli >= 15000 && milli <= 125000) {
                            const deg = Math.round(milli / 1000);
                            if (prio > bestPrio || (prio === bestPrio && deg > (bestTemp || 0))) {
                                bestPrio = prio;
                                bestTemp = deg;
                            }
                        }
                    }
                }
            }
        } catch (e) {}

        if (bestTemp !== null) {
            this._tempLabel.text = `TMP ${bestTemp}°C`;
            this._menuStatTemp.label.text = `Temperature: ${bestTemp}°C`;
        } else {
            this._tempLabel.text = `TMP —°C`;
        }

        // 3. RAM
        try {
            const memText = GLib.file_get_contents('/proc/meminfo')[1];
            const lines = new TextDecoder().decode(memText).split('\n');
            let total = 0, avail = 0;
            for (const l of lines) {
                if (l.startsWith('MemTotal:')) total = parseInt(l.split(/\s+/)[1], 10) * 1024;
                if (l.startsWith('MemAvailable:')) avail = parseInt(l.split(/\s+/)[1], 10) * 1024;
            }
            if (total > 0) {
                const used = total - avail;
                const pct = Math.round((used / total) * 100);
                this._ramLabel.text = `RAM ${pct}%`;
                const usedGb = (used / (1024 * 1024 * 1024)).toFixed(1);
                const totalGb = (total / (1024 * 1024 * 1024)).toFixed(1);
                this._menuStatRam.label.text = `RAM: ${usedGb} / ${totalGb} GB (${pct}%)`;
            }
        } catch (e) {}

        // 4. Network — prefer the IPv4 default-route interface so Docker/veth
        // counters are not summed into the desktop throughput indicator.
        try {
            let preferredIface = null;
            try {
                const routeText = new TextDecoder().decode(GLib.file_get_contents('/proc/net/route')[1]);
                for (const line of routeText.split('\n').slice(1)) {
                    const fields = line.trim().split(/\s+/);
                    if (fields.length > 2 && fields[1] === '00000000') {
                        preferredIface = fields[0];
                        break;
                    }
                }
            } catch (e) {}

            const devText = GLib.file_get_contents('/proc/net/dev')[1];
            const lines = new TextDecoder().decode(devText).split('\n').slice(2);
            let totalRx = 0, totalTx = 0;
            let matchedPreferred = false;
            for (const l of lines) {
                if (!l.includes(':')) continue;
                const [nameText, statsText] = l.split(':', 2);
                const name = nameText.trim();
                if (name === 'lo') continue;
                if (preferredIface && name !== preferredIface) continue;
                const parts = statsText.trim().split(/\s+/).map(Number);
                if (parts.length >= 9) {
                    totalRx += parts[0] || 0;
                    totalTx += parts[8] || 0;
                    matchedPreferred = preferredIface ? true : matchedPreferred;
                }
            }

            // If route discovery failed, sum non-loopback interfaces as a fallback.
            if (preferredIface && !matchedPreferred) {
                totalRx = 0;
                totalTx = 0;
                for (const l of lines) {
                    if (!l.includes(':')) continue;
                    const [nameText, statsText] = l.split(':', 2);
                    if (nameText.trim() === 'lo') continue;
                    const parts = statsText.trim().split(/\s+/).map(Number);
                    if (parts.length >= 9) {
                        totalRx += parts[0] || 0;
                        totalTx += parts[8] || 0;
                    }
                }
            }

            const now = GLib.get_monotonic_time();
            if (this._prevNet) {
                const dt = (now - this._prevNet.time) / 1000000;
                if (dt > 0.1) {
                    const downSpeed = Math.max(0, (totalRx - this._prevNet.rx) / dt);
                    const upSpeed = Math.max(0, (totalTx - this._prevNet.tx) / dt);
                    const formatRate = (b) => {
                        if (b > 1024 * 1024) return `${(b / (1024 * 1024)).toFixed(1)} M/s`;
                        if (b > 1024) return `${Math.round(b / 1024)} K/s`;
                        return `${Math.round(b)} B/s`;
                    };
                    this._netLabel.text = `NET ${formatRate(downSpeed)}`;
                    this._menuStatNet.label.text = `Network: ↓ ${formatRate(downSpeed)} • ↑ ${formatRate(upSpeed)}`;
                }
            }
            this._prevNet = { rx: totalRx, tx: totalTx, time: now };
        } catch (e) {}

        // 5. Battery — discover Battery-type power supplies instead of assuming BAT0.
        try {
            let batteryName = null;
            const root = Gio.File.new_for_path('/sys/class/power_supply');
            const enumerator = root.enumerate_children('standard::name', Gio.FileQueryInfoFlags.NONE, null);
            let info;
            while ((info = enumerator.next_file(null)) !== null) {
                const name = info.get_name();
                const typePath = `/sys/class/power_supply/${name}/type`;
                if (!GLib.file_test(typePath, GLib.FileTest.EXISTS)) continue;
                const type = new TextDecoder().decode(GLib.file_get_contents(typePath)[1]).trim();
                if (type === 'Battery') {
                    batteryName = name;
                    break;
                }
            }
            enumerator.close(null);

            if (batteryName) {
                const capPath = `/sys/class/power_supply/${batteryName}/capacity`;
                const statPath = `/sys/class/power_supply/${batteryName}/status`;
                if (GLib.file_test(capPath, GLib.FileTest.EXISTS)) {
                    const cap = new TextDecoder().decode(GLib.file_get_contents(capPath)[1]).trim();
                    const status = GLib.file_test(statPath, GLib.FileTest.EXISTS)
                        ? new TextDecoder().decode(GLib.file_get_contents(statPath)[1]).trim()
                        : '';
                    const prefix = status === 'Charging' ? 'BAT+' : 'BAT';
                    this._batLabel.text = `${prefix} ${cap}%`;
                    this._menuStatBat.label.text = `Battery: ${cap}% (${status || 'Unknown'})`;
                }
            } else {
                this._batLabel.text = 'BAT —';
                this._menuStatBat.label.text = 'Battery: not detected';
            }
        } catch (e) {
            this._batLabel.text = 'BAT —';
        }
    }

    _setupConfigMonitor() {
        try {
            const dirPath = GLib.build_filenamev([GLib.get_user_config_dir(), 'linuxcare']);
            GLib.mkdir_with_parents(dirPath, 0o700);
            const dir = Gio.File.new_for_path(dirPath);
            this._fileMonitor = dir.monitor_directory(Gio.FileMonitorFlags.NONE, null);
            this._fileMonitor.connect('changed', (mon, file, other, event) => {
                if (!file || file.get_basename() !== 'config.json')
                    return;
                if (event === Gio.FileMonitorEvent.CHANGES_DONE_HINT ||
                    event === Gio.FileMonitorEvent.CREATED ||
                    event === Gio.FileMonitorEvent.MOVED_IN) {
                    this._loadConfig();
                    this._applyConfigVisibility();
                    this._startTimer();
                }
            });
        } catch (e) {}
    }
}
