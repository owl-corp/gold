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
