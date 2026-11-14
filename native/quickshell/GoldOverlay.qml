import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Wayland

// Thin view for the gold daemon: it renders whatever the daemon sends and
// reports `closed` when dismissed. All state and logic live in Rust.
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

    Process {
        id: opener

        function open(url) {
            command = ["xdg-open", url];
            running = true;
        }
    }

    function close() {
        root.view = null;
        sock.write(JSON.stringify({ type: "closed" }) + "\n");
        sock.flush();
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

                x: Math.min(root.view?.x ?? 0, win.width - width)
                y: Math.min(root.view?.y ?? 0, win.height - height)
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
                        StyledText { required property string modelData; text: modelData }
                    }
                }
            }
        }
    }
}
