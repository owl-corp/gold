import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Thin view for the gold daemon: it renders whatever the daemon sends and sends
// the user's clicks back as actions. All state and logic live in Rust.
Scope {
    id: root

    property var view: null
    // The lists below only change when their content does, so a row being edited
    // keeps its focus and typed text while a search result arrives.
    property var filterRows: []
    property var presetRows: []
    property var noteLines: []
    property var groups: []
    // a value box has keyboard focus: do not close the panel under the user's hands
    property bool editing: false
    // epoch of the view the user last closed; a late view for it must not reopen the panel
    property var closedEpoch: null
    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + (Quickshell.env("GOLD_DEV") ? "/gold-dev" : "/gold") + "/ui.sock"

    function rarityColor(rarity) {
        switch (rarity) {
        case "magic": return "#8888ff";
        case "rare": return "#ffff77";
        case "unique": return "#af6025";
        case "currency": return "#aa9e82";
        default: return Theme.text;
        }
    }

    function statusColor(status) {
        return status === "online" ? Theme.green : status === "afk" ? Theme.yellow : Theme.textDim;
    }

    function keep(name, value) {
        if (JSON.stringify(root[name]) !== JSON.stringify(value)) root[name] = value;
    }

    onViewChanged: {
        keep("filterRows", view?.filters ?? []);
        keep("presetRows", view?.presets ?? []);
        keep("noteLines", view?.lines ?? []);
        keep("groups", view?.trade?.state === "done" ? view.trade.groups : []);
        if (view === null) editing = false;
    }

    function send(action) {
        // actions say which view they were clicked in, so a stale click is ignored
        action.epoch = root.view?.epoch;
        sock.write(JSON.stringify(action) + "\n");
        sock.flush();
    }

    function close() {
        root.closedEpoch = root.view?.epoch ?? null;
        root.view = null;
        sock.write(JSON.stringify({ type: "closed" }) + "\n");
        sock.flush();
    }

    // number from a text field, or null when empty/invalid
    function parseNumber(text) {
        const n = parseFloat(text);
        return isNaN(n) ? null : n;
    }

    Process {
        id: opener

        function open(url) {
            command = ["xdg-open", url];
            running = true;
        }
    }

    Socket {
        id: sock

        path: root.socketPath
        connected: true
        // Don't leave a stale panel up when the daemon goes away.
        onConnectedChanged: if (!connected) root.view = null

        parser: SplitParser {
            onRead: line => {
                const msg = JSON.parse(line);
                if (msg.type === "show") {
                    if (msg.view.epoch === undefined || msg.view.epoch !== root.closedEpoch) root.view = msg.view;
                }
                else if (msg.type === "hide") root.view = null;
            }
        }
    }

    Timer {
        interval: 2000
        repeat: true
        running: !sock.connected
        onTriggered: sock.connected = true
    }

    // The window exists only while a view is shown, so nothing can eat clicks when idle.
    LazyLoader {
        active: root.view !== null

        PanelWindow {
            id: win

            // Cover the screen the daemon names (else the first one). The mask limits
            // input to the panel, so clicks elsewhere reach the game.
            screen: Quickshell.screens.find(s => s.name === root.view?.screen) ?? Quickshell.screens[0]
            anchors { top: true; bottom: true; left: true; right: true }
            color: "transparent"
            exclusionMode: ExclusionMode.Ignore
            mask: Region { item: panel }

            WlrLayershell.namespace: "gold-overlay"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.OnDemand

            Rectangle {
                id: panel

                x: Math.max(0, Math.min(root.view?.x ?? 0, win.width - width))
                y: Math.max(0, Math.min(root.view?.y ?? 0, win.height - height))
                implicitWidth: col.implicitWidth + 28
                implicitHeight: col.implicitHeight + 24
                radius: 16
                color: Theme.surface
                border.color: Qt.alpha(Theme.text, 0.08)
                focus: true

                Keys.onEscapePressed: root.close()

                // Close shortly after the pointer leaves. Only once it has been
                // inside (the panel opens under the cursor, but not always), and never while typing.
                property bool entered: false

                HoverHandler {
                    id: hover

                    onHoveredChanged: {
                        if (hovered) {
                            panel.entered = true;
                            leaveTimer.stop();
                        } else if (panel.entered) leaveTimer.restart();
                    }
                }
                Timer {
                    id: leaveTimer

                    interval: 800
                    onTriggered: if (!hover.hovered && !root.editing) root.close()
                }

                ColumnLayout {
                    id: col

                    anchors.centerIn: parent
                    spacing: 6

                    StyledText {
                        text: root.view?.title ?? ""
                        font.bold: true
                        font.pixelSize: Theme.fontSize + 2
                        color: root.rarityColor(root.view?.rarity)
                    }
                    StyledText {
                        visible: text.length > 0
                        text: root.view?.subtitle ?? ""
                        color: Theme.textDim
                    }

                    // price from poe.ninja, or why there is none
                    RowLayout {
                        visible: root.view?.price != null
                        spacing: 8

                        StyledText {
                            visible: root.view?.price?.amount != null
                            text: (root.view?.price?.amount ?? "") + " " + (root.view?.price?.unit ?? "")
                            font.bold: true
                            font.pixelSize: Theme.fontSize + 2
                            color: Theme.yellow
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: opener.open(root.view.price.url)
                            }
                        }
                        StyledText {
                            visible: root.view?.price?.trendText != null
                            text: (root.view?.price?.trend === "up" ? "▲ " : root.view?.price?.trend === "down" ? "▼ " : "± ") + (root.view?.price?.trendText ?? "")
                            color: root.view?.price?.trend === "up" ? Theme.green : root.view?.price?.trend === "down" ? Theme.red : Theme.textDim
                        }
                        StyledText {
                            visible: root.view?.price?.average != null
                            text: "(" + (root.view?.price?.average ?? "") + ")"
                            color: Theme.textDim
                        }
                        StyledText {
                            visible: root.view?.price?.status != null
                            text: root.view?.price?.status ?? ""
                            color: Theme.textDim
                        }
                    }

                    // related items, or every price of a small poe.ninja category:
                    // one block per group, a marker / name / price grid so the prices line up
                    Repeater {
                        model: root.view?.price?.groups ?? []

                        ColumnLayout {
                            id: group

                            required property var modelData

                            Layout.fillWidth: true
                            Layout.topMargin: 4
                            spacing: 4

                            Rectangle {
                                Layout.fillWidth: true
                                implicitHeight: 1
                                color: Qt.alpha(Theme.text, 0.1)
                            }
                            StyledText {
                                visible: !!group.modelData.heading
                                text: (group.modelData.heading ?? "").toUpperCase()
                                font.pixelSize: Theme.fontSize - 2
                                font.letterSpacing: 1
                                color: Theme.textDim
                            }
                            GridLayout {
                                // the three Repeaters fill the grid column by column
                                flow: GridLayout.TopToBottom
                                rows: group.modelData.rows.length
                                columnSpacing: 8
                                rowSpacing: 3

                                Repeater {
                                    model: group.modelData.rows
                                    Rectangle {
                                        required property var modelData
                                        Layout.fillHeight: true
                                        implicitWidth: 3
                                        radius: 2
                                        color: modelData.current ? Theme.accent : "transparent"
                                    }
                                }
                                Repeater {
                                    model: group.modelData.rows
                                    StyledText {
                                        required property var modelData
                                        Layout.fillWidth: true
                                        text: modelData.name
                                        font.bold: modelData.current
                                        color: modelData.current ? Theme.text : Theme.textDim
                                    }
                                }
                                Repeater {
                                    model: group.modelData.rows
                                    StyledText {
                                        id: priceText

                                        required property var modelData
                                        Layout.alignment: Qt.AlignRight
                                        Layout.leftMargin: 14
                                        text: modelData.amount != null ? modelData.amount + " " + modelData.unit : "—"
                                        font.bold: modelData.current
                                        font.underline: priceHit.containsMouse && modelData.url != null
                                        color: modelData.amount != null ? Theme.yellow : Theme.textDim
                                        MouseArea {
                                            id: priceHit

                                            anchors.fill: parent
                                            hoverEnabled: true
                                            enabled: priceText.modelData.url != null
                                            cursorShape: Qt.PointingHandCursor
                                            onClicked: opener.open(priceText.modelData.url)
                                        }
                                    }
                                }
                            }
                        }
                    }

                    Repeater {
                        model: root.noteLines
                        StyledText { required property string modelData; text: modelData; color: Theme.textDim }
                    }

                    // presets: similar items / exact / bulk ...
                    RowLayout {
                        visible: (root.view?.presets?.length ?? 0) > 1
                        spacing: 10

                        Repeater {
                            model: root.presetRows

                            StyledText {
                                required property var modelData

                                text: modelData.name
                                font.bold: modelData.active
                                color: modelData.active ? Theme.accent : Theme.textDim
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: { panel.forceActiveFocus(); root.send({ type: "preset", index: modelData.index }); }
                                }
                            }
                        }
                    }

                    // one row per stat filter
                    Repeater {
                        model: root.filterRows

                        RowLayout {
                            id: row

                            required property var modelData

                            spacing: 8

                            Rectangle {
                                implicitWidth: 16
                                implicitHeight: 16
                                radius: 4
                                color: row.modelData.on ? Theme.accent : "transparent"
                                border.color: Qt.alpha(Theme.text, 0.4)
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: { panel.forceActiveFocus(); root.send({ type: "toggle", index: row.modelData.index }); }
                                }
                            }
                            StyledText {
                                visible: row.modelData.tag === "fractured"
                                text: "Fractured"
                                color: "#d4c08a"
                                font.bold: true
                            }
                            StyledText {
                                Layout.fillWidth: true
                                Layout.maximumWidth: 360
                                elide: Text.ElideRight
                                text: (row.modelData.not ? "NOT " : "") + row.modelData.text
                                color: row.modelData.on ? Theme.text : Theme.textDim
                            }
                            Repeater {
                                model: row.modelData.editable ? ["min", "max"] : []

                                Rectangle {
                                    id: box

                                    required property string modelData

                                    implicitWidth: 52
                                    implicitHeight: 22
                                    radius: 6
                                    color: Qt.alpha(Theme.text, 0.08)

                                    TextInput {
                                        id: input

                                        anchors.fill: parent
                                        anchors.margins: 4
                                        color: Theme.text
                                        font.family: Theme.font
                                        font.pixelSize: Theme.fontSize
                                        horizontalAlignment: TextInput.AlignHCenter
                                        selectByMouse: true
                                        text: row.modelData[box.modelData] != null ? String(row.modelData[box.modelData]) : ""
                                        Keys.onEscapePressed: root.close()
                                        onActiveFocusChanged: {
                                            root.editing = activeFocus;
                                            if (!activeFocus && !hover.hovered && panel.entered) leaveTimer.restart();
                                        }
                                        onEditingFinished: {
                                            const other = box.modelData === "min" ? "max" : "min";
                                            const edited = root.parseNumber(text);
                                            // focus loss also ends editing: nothing changed, nothing to send
                                            if (edited === (row.modelData[box.modelData] ?? null)) return;
                                            const current = row.modelData[other];
                                            root.send({
                                                type: "set",
                                                index: row.modelData.index,
                                                min: box.modelData === "min" ? edited : current,
                                                max: box.modelData === "max" ? edited : current
                                            });
                                        }
                                    }
                                    StyledText {
                                        visible: input.text.length === 0 && !input.activeFocus
                                        anchors.centerIn: parent
                                        text: box.modelData
                                        color: Theme.textDim
                                        font.pixelSize: Theme.fontSize - 2
                                    }
                                }
                            }
                        }
                    }

                    StyledText {
                        visible: (root.view?.hiddenCount ?? 0) > 0 || root.view?.showHidden === true
                        text: root.view?.showHidden ? "hide unused filters" : "show " + root.view?.hiddenCount + " hidden filters"
                        color: Theme.blue
                        MouseArea {
                            anchors.fill: parent
                            cursorShape: Qt.PointingHandCursor
                            onClicked: { panel.forceActiveFocus(); root.send({ type: "hidden" }); }
                        }
                    }

                    StyledText {
                        visible: root.view?.searchQuality != null
                        text: "quality ≥ " + root.view?.searchQuality + "%"
                        color: Theme.accent
                    }

                    // how the trade search is made; changing one runs the search again
                    RowLayout {
                        visible: root.view?.tradeFilters != null && (root.view?.presets?.length ?? 0) > 0
                        spacing: 10

                        Repeater {
                            model: {
                                const t = root.view?.tradeFilters;
                                if (!t) return [];
                                const listed = { "1day": "1 day", "3days": "3 days", "1week": "1 week", "2weeks": "2 weeks", "1month": "1 month", "2months": "2 months" };
                                return [
                                    ...(t.magicOnly != null ? [{ key: "magicOnly", text: "magic only", on: t.magicOnly }] : []),
                                    { key: "offline", text: t.offline ? "online + offline" : "online only", on: t.offline },
                                    { key: "merchantOnly", text: "instant buyout", on: t.merchantOnly },
                                    { key: "listed", text: "listed: " + (listed[t.listed] ?? "any time"), on: t.listed != null }
                                ];
                            }

                            StyledText {
                                required property var modelData

                                text: modelData.text
                                color: modelData.on ? Theme.accent : Theme.textDim
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: { panel.forceActiveFocus(); root.send({ type: "trade", key: modelData.key }); }
                                }
                            }
                        }
                    }

                    // search
                    RowLayout {
                        visible: (root.view?.presets?.length ?? 0) > 0
                        spacing: 10

                        Rectangle {
                            implicitWidth: searchLabel.implicitWidth + 24
                            implicitHeight: 26
                            radius: 8
                            color: Theme.accent
                            opacity: root.view?.trade?.state === "loading" ? 0.5 : 1

                            StyledText {
                                id: searchLabel

                                anchors.centerIn: parent
                                text: root.view?.trade?.state === "loading" ? "Searching…" : "Search"
                                color: Theme.accentText
                                font.bold: true
                            }
                            MouseArea {
                                anchors.fill: parent
                                enabled: root.view?.trade?.state !== "loading"
                                cursorShape: Qt.PointingHandCursor
                                onClicked: {
                                    // take focus off a box being edited, so its value is sent first
                                    panel.forceActiveFocus();
                                    root.send({ type: "search" });
                                }
                            }
                        }
                        StyledText {
                            visible: root.view?.trade?.state === "failed"
                            text: root.view?.trade?.message ?? ""
                            color: Theme.red
                            Layout.maximumWidth: 320
                            wrapMode: Text.Wrap
                        }
                    }

                    // results
                    Repeater {
                        model: root.groups

                        ColumnLayout {
                            id: group

                            required property var modelData

                            spacing: 2

                            RowLayout {
                                spacing: 8

                                StyledText {
                                    text: group.modelData.total + " listed" + (group.modelData.heading ? " " + group.modelData.heading : "")
                                    color: Theme.textDim
                                }
                                StyledText {
                                    text: "open on site"
                                    color: Theme.blue
                                    MouseArea {
                                        anchors.fill: parent
                                        cursorShape: Qt.PointingHandCursor
                                        onClicked: opener.open(group.modelData.url)
                                    }
                                }
                            }
                            StyledText {
                                visible: group.modelData.rows.length === 0
                                text: "No listings"
                                color: Theme.textDim
                            }
                            Repeater {
                                id: listingRows

                                model: group.modelData.rows

                                RowLayout {
                                    id: listing

                                    required property var modelData

                                    spacing: 8

                                    Rectangle {
                                        implicitWidth: 8
                                        implicitHeight: 8
                                        radius: 4
                                        color: root.statusColor(listing.modelData.status)
                                    }
                                    StyledText {
                                        text: listing.modelData.price
                                        font.bold: true
                                        color: Theme.yellow
                                        Layout.minimumWidth: 70
                                    }
                                    StyledText {
                                        text: listing.modelData.detail
                                        color: Theme.textDim
                                    }
                                    StyledText {
                                        text: listing.modelData.who + (listing.modelData.mine ? " (you)" : "")
                                        color: listing.modelData.mine ? Theme.yellow : Theme.textDim
                                    }
                                    StyledText {
                                        visible: listing.modelData.count > 1
                                        text: "×" + listing.modelData.count
                                        color: Theme.text
                                    }
                                }
                            }
                            StyledText {
                                visible: group.modelData.more === true
                                text: group.modelData.fetchingMore ? "Loading…" : "show more"
                                color: Theme.blue
                                MouseArea {
                                    anchors.fill: parent
                                    enabled: !group.modelData.fetchingMore
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: { panel.forceActiveFocus(); root.send({ type: "more" }); }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
