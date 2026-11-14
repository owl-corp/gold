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
    readonly property string socketPath: Quickshell.env("XDG_RUNTIME_DIR") + "/gold/ui.sock"

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

    function send(action) {
        sock.write(JSON.stringify(action) + "\n");
        sock.flush();
    }

    function close() {
        root.view = null;
        send({ type: "closed" });
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
                if (msg.type === "show") root.view = msg.view;
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
                            visible: root.view?.price?.status != null
                            text: root.view?.price?.status ?? ""
                            color: Theme.textDim
                        }
                    }

                    Repeater {
                        model: root.view?.lines ?? []
                        StyledText { required property string modelData; text: modelData; color: Theme.textDim }
                    }

                    // presets: similar items / exact / bulk ...
                    RowLayout {
                        visible: (root.view?.presets?.length ?? 0) > 1
                        spacing: 10

                        Repeater {
                            model: root.view?.presets ?? []

                            StyledText {
                                required property var modelData

                                text: modelData.name
                                font.bold: modelData.active
                                color: modelData.active ? Theme.accent : Theme.textDim
                                MouseArea {
                                    anchors.fill: parent
                                    cursorShape: Qt.PointingHandCursor
                                    onClicked: root.send({ type: "preset", index: modelData.index })
                                }
                            }
                        }
                    }

                    // one row per stat filter
                    Repeater {
                        model: root.view?.filters ?? []

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
                                    onClicked: root.send({ type: "toggle", index: row.modelData.index })
                                }
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
                                        onEditingFinished: {
                                            const other = box.modelData === "min" ? "max" : "min";
                                            const edited = root.parseNumber(text);
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
                                onClicked: root.send({ type: "search" })
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
                        model: root.view?.trade?.state === "done" ? root.view.trade.groups : []

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
                                        text: listing.modelData.who
                                        color: Theme.textDim
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
