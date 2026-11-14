pragma Singleton
import QtQuick
import Quickshell

// Dev stub; the real overlay uses the Theme singleton from the user's shell config.
Singleton {
    readonly property color surface: "#f23d1a39"
    readonly property color text: "#ffffff"
    readonly property string font: "sans-serif"
    readonly property int fontSize: 13
    readonly property int anim: 250
}
