import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Window

ApplicationWindow {
    id: root
    visible: true
    width: Screen.width
    height: expanded ? 520 : 58
    x: 0
    y: 0
    color: "transparent"
    flags: Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint | Qt.Tool
    property bool expanded: false

    Behavior on height { SpringAnimation { spring: 3.2; damping: 0.28; mass: 0.8 } }

    Rectangle {
        anchors.top: parent.top
        width: parent.width
        height: parent.height
        radius: 22
        color: "#dd151821"
        border.color: "#5540e0d0"
        clip: true

        MouseArea {
            anchors.fill: header
            onClicked: root.expanded = !root.expanded
        }
        Rectangle {
            id: header
            width: parent.width
            height: 58
            color: "#e9161a25"
            RowLayout {
                anchors.fill: parent; anchors.leftMargin: 26; anchors.rightMargin: 26
                Text { text: "VKPL"; color: "#8ff5e9"; font.bold: true; font.pixelSize: 18 }
                Text { text: "now playing"; color: "#929bb7"; font.pixelSize: 14 }
                Item { Layout.fillWidth: true }
                Text { text: root.expanded ? "⌃" : "⌄"; color: "#d7ddf0"; font.pixelSize: 24 }
            }
        }

        RowLayout {
            anchors.top: header.bottom; anchors.topMargin: 26
            anchors.left: parent.left; anchors.right: parent.right; anchors.margins: 34
            spacing: 26
            Rectangle { Layout.preferredWidth: 250; Layout.preferredHeight: 250; radius: 18; color: "#423c91ff"
                Text { anchors.centerIn: parent; text: "♫"; font.pixelSize: 92; color: "#d5ccff" } }
            ColumnLayout { Layout.fillWidth: true; spacing: 14
                Text { text: "Разные с тобой"; color: "#f1f4ff"; font.pixelSize: 32; font.bold: true }
                Text { text: "emokid"; color: "#f49ed9"; font.pixelSize: 19 }
                Slider { Layout.fillWidth: true; value: 0.24 }
                RowLayout { spacing: 18
                    Button { text: "‹‹" }; Button { text: "▶"; font.pixelSize: 21 }; Button { text: "››" }
                    Text { text: "0:38 / 2:46"; color: "#aab4d3" }
                }
            }
            ColumnLayout { Layout.preferredWidth: 300
                Text { text: "Очередь"; color: "#75d8ff"; font.bold: true; font.pixelSize: 18 }
                Repeater { model: ["Дождик идет — Чучело", "Я ушел гулять — Bisexual", "Под дезоморфином — Bisexual"]
                    delegate: Text { text: (index + 1) + ".  " + modelData; color: index === 0 ? "#91f7c3" : "#b7c0dc"; padding: 7 } }
            }
        }
    }
}
