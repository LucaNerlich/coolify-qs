import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Native Quattro popup for Coolify deployments: current and recent
// deployments grouped by server and application. State flows in from
// BarWidget.qml (fed by the Rust watch stream).
Panel {
  id: root
  moduleName: "luca.coolify"
  manageIpc: false

  property var anchorItem: null
  property var hostWidget: null
  readonly property var barIdentity: hostWidget || root
  readonly property var watcher: hostWidget || root
  readonly property bool hasWatcher: watcher !== null && watcher !== root

  readonly property color foreground: bar ? bar.foreground : Color.foreground
  readonly property color urgent: bar ? bar.urgent : Color.urgent
  readonly property color active: Color.bar.active
  readonly property string fontFamily: bar ? bar.fontFamily : Style.font.family

  readonly property var status: hasWatcher ? (watcher.status || null) : null
  readonly property bool isError: status !== null && status.state === "error"
  readonly property var servers: status !== null && status.state === "ok"
    ? status.servers : []

  function openUrl(url) {
    if (!hasWatcher || typeof watcher.openUrl !== "function") return
    watcher.openUrl(url)
  }

  function switchPanel(direction) {
    if (root.bar && typeof root.bar.switchPanelFrom === "function")
      return root.bar.switchPanelFrom(root.barIdentity, direction)
    return false
  }

  KeyboardPanel {
    id: panel
    anchorItem: root.anchorItem
    owner: root
    bar: root.bar
    open: root.opened
    focusTarget: keyCatcher
    contentWidth: panel.fittedContentWidth(Style.space(380))
    contentHeight: panel.fittedContentHeight(column.implicitHeight, Style.space(560))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
    }

    Flickable {
      id: panelFlick
      anchors.fill: parent
      contentWidth: width
      contentHeight: column.implicitHeight
      clip: true
      boundsBehavior: Flickable.StopAtBounds
      flickableDirection: Flickable.VerticalFlick
      interactive: contentHeight > height
      ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

      Column {
        id: column
        width: panelFlick.width
        spacing: Style.space(10)

        PanelHero {
          width: parent.width
          title: "Coolify"
          meta: Model.metaLine(root.status)
          detail: root.isError
            ? (status.error || "Config error")
            : "Deployments across your Coolify servers"
          foreground: root.foreground
          fontFamily: root.fontFamily

          iconComponent: Component {
            Text {
              text: "\uD83D\uDE80"
              textFormat: Text.PlainText
              color: root.isError ? root.urgent : root.foreground
              font.family: root.fontFamily
              font.pixelSize: Style.font.display
            }
          }
        }

        Text {
          width: parent.width
          visible: root.isError
          text: "Create ~/.config/coolify-qs/config.json with your servers and tokens."
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.4)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          wrapMode: Text.WordWrap
        }

        Text {
          width: parent.width
          visible: root.status !== null && !root.isError && root.servers.length === 0
          text: "No servers configured."
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.4)
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
        }

        Repeater {
          model: root.servers
          delegate: serverDelegate
        }
      }
    }
  }

  Component {
    id: serverDelegate
    Column {
      width: column.width
      spacing: Style.space(4)

      PanelSectionHeader {
        width: parent.width
        foreground: root.foreground
        fontFamily: root.fontFamily
        text: (modelData.online ? "" : "\u2298 ") + modelData.name
          + (modelData.online ? "" : " \u2014 offline")
      }

      Text {
        width: parent.width
        text: Model.serverLine(modelData)
        textFormat: Text.PlainText
        color: Qt.darker(root.foreground, 1.4)
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        elide: Text.ElideRight
        wrapMode: Text.NoWrap
        MouseArea {
          anchors.fill: parent
          cursorShape: Qt.PointingHandCursor
          onClicked: root.openUrl(modelData.url)
        }
      }

      Text {
        width: parent.width
        visible: modelData.online !== true && !!modelData.error
        text: modelData.error || ""
        textFormat: Text.PlainText
        color: root.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
      }

      Repeater {
        model: modelData.online ? modelData.apps : []
        delegate: appDelegate
      }
    }
  }

  Component {
    id: appDelegate
    Column {
      width: column.width
      spacing: Style.space(2)
      leftPadding: Style.space(16)

      RowLayout {
        width: parent.width
        spacing: Style.space(8)

        Text {
          Layout.maximumWidth: parent.width * 0.55
          text: modelData.name || modelData.uuid || "application"
          textFormat: Text.PlainText
          color: root.foreground
          font.family: root.fontFamily
          font.pixelSize: Style.font.body
          font.bold: true
          elide: Text.ElideRight
          wrapMode: Text.NoWrap
        }

        Text {
          visible: !!modelData.fqdn
          text: modelData.fqdn || ""
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.4)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
          wrapMode: Text.NoWrap
        }
      }

      Repeater {
        model: modelData.deployments || []
        delegate: deploymentDelegate
      }

      Text {
        visible: (modelData.deployments || []).length === 0
        text: "no deployments yet"
        textFormat: Text.PlainText
        color: Qt.darker(root.foreground, 1.5)
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }
    }
  }

  Component {
    id: deploymentDelegate
    RowLayout {
      width: column.width
      spacing: Style.space(8)

      Text {
        text: Model.statusGlyph(modelData.status)
        textFormat: Text.PlainText
        color: Model.isActive(modelData.status) ? root.active
          : (modelData.status === "failed" ? root.urgent
            : Qt.darker(root.foreground, 1.4))
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
      }

      Text {
        Layout.fillWidth: true
        text: Model.deploymentText(modelData)
        textFormat: Text.PlainText
        color: Model.isActive(modelData.status) ? root.foreground
          : Qt.darker(root.foreground, 1.4)
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        elide: Text.ElideRight
        wrapMode: Text.NoWrap
      }

      Text {
        text: Model.relativeTime(modelData.createdAt)
        textFormat: Text.PlainText
        color: Qt.darker(root.foreground, 1.5)
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
      }

      MouseArea {
        anchors.fill: parent
        cursorShape: modelData.deploymentUrl ? Qt.PointingHandCursor : Qt.ArrowCursor
        enabled: !!modelData.deploymentUrl
        onClicked: root.openUrl(modelData.deploymentUrl)
      }
    }
  }
}
