pragma Singleton
import QtQuick
import Quickshell

// Dev stub; the real overlay uses the Theme singleton from the user's shell config.
Singleton {
    readonly property color surface: "#f23d1a39"
    readonly property color text: "#ffffff"
    readonly property color blue: "#5aa9ff"
    readonly property color accentText: "#2d112a"
    readonly property color accent: "#ff6699"
    readonly property color textDim: "#99ffffff"
    readonly property color green: "#7ee0b8"
    readonly property color yellow: "#ffcc66"
    readonly property color red: "#ff4d6d"
    readonly property string font: "sans-serif"
    readonly property int fontSize: 13
    readonly property int anim: 250
}
