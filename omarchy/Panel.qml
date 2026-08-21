import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import Quickshell
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Native Quattro popup for Coolify deployments: current and recent
// deployments in one column per server. State flows in from BarWidget.qml
// (fed by the Rust watch stream).
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

  // Per-server column width. Long commit messages elide inside it.
  readonly property int serverColumnWidth: Style.space(300)

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
    // Grow with the number of servers (clamped to the screen by
    // fittedContentWidth), so one column fits without eliding.
    contentWidth: panel.fittedContentWidth(
      Math.max(Style.space(340), serversRow.implicitWidth))
    contentHeight: panel.fittedContentHeight(
      column.implicitHeight, Style.space(560))

    PanelKeyCatcher {
      id: keyCatcher
      anchors.fill: parent
      onCloseRequested: root.close()
      onTabRequested: function(direction) { root.switchPanel(direction) }
      onMoveRequested: function(dx, dy) {
        if (dy === 0) return
        panelFlick.contentY = Math.max(0, Math.min(
          panelFlick.contentY + dy * Style.space(44),
          Math.max(0, panelFlick.contentHeight - panelFlick.height)))
      }

      Flickable {
        id: panelFlick
        anchors.fill: parent
        contentWidth: column.implicitWidth
        contentHeight: column.implicitHeight
        clip: true
        boundsBehavior: Flickable.StopAtBounds
        flickableDirection: Flickable.HorizontalAndVerticalFlick
        interactive: contentHeight > height || contentWidth > width
        ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }
        ScrollBar.horizontal: ScrollBar { policy: ScrollBar.AsNeeded }

        Column {
          id: column
          width: Math.max(panelFlick.width, panelFlick.contentWidth)
          spacing: Style.space(10)

          PanelHero {
            width: parent.width
            title: "Coolify"
            meta: Model.metaLine(root.status)
            // PanelHero renders with AutoText, which only parses markup on
            // a raw "<"; strip markup characters so error text shows
            // literally, and cap the length so the detail pill cannot
            // grow past the panel.
            detail: Model.shortName(Model.stripMarkup(root.isError
              ? (status.error || "Config error")
              : "Deployments across your Coolify servers"), 60)
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

          Row {
            id: serversRow
            spacing: Style.space(16)
            visible: root.servers.length > 0

            Repeater {
              model: root.servers
              delegate: serverColumnDelegate
            }
          }
        }
      }
    }
  }

  Component {
    id: serverColumnDelegate
    Column {
      id: serverColumn
      width: root.serverColumnWidth
      spacing: Style.space(6)

      readonly property var shownApps: Model.appsWithDeployments(modelData.apps)
      readonly property int hiddenAppCount: (modelData.apps || []).length - shownApps.length

      PanelSectionHeader {
        width: parent.width
        foreground: root.foreground
        fontFamily: root.fontFamily
        // PanelSectionHeader is a plain Text (no rich text, no elide of its
        // own). Strip markup characters and elide here so long server names
        // cannot paint into the neighboring server column.
        text: Model.stripMarkup((modelData.online ? "" : "\u2298 ") + modelData.name
          + (modelData.online ? "" : " \u2014 offline"))
        elide: Text.ElideRight
        wrapMode: Text.NoWrap
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
        model: serverColumn.shownApps
        delegate: appDelegate
      }

      Text {
        width: parent.width
        visible: serverColumn.hiddenAppCount > 0
        text: serverColumn.hiddenAppCount === 1
          ? "1 app without deployments"
          : serverColumn.hiddenAppCount + " apps without deployments"
        textFormat: Text.PlainText
        color: Qt.darker(root.foreground, 1.5)
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
      }
    }
  }

  Component {
    id: appDelegate
    Column {
      width: root.serverColumnWidth
      spacing: Style.space(2)
      topPadding: Style.space(6)

      PanelSeparator {
        width: parent.width
        foreground: root.foreground
      }

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
          // Fill the remaining row width so elide actually activates for
          // long fqdns instead of painting past the 300 px column.
          Layout.fillWidth: true
          text: modelData.fqdn || ""
          textFormat: Text.PlainText
          color: Qt.darker(root.foreground, 1.4)
          font.family: root.fontFamily
          font.pixelSize: Style.font.caption
          elide: Text.ElideRight
          wrapMode: Text.NoWrap
        }
      }

      Text {
        width: parent.width
        visible: !(modelData.deployments && modelData.deployments.length > 0)
          && !!modelData.error
        text: Model.shortName(modelData.error || "", 200)
        textFormat: Text.PlainText
        color: root.urgent
        font.family: root.fontFamily
        font.pixelSize: Style.font.caption
        wrapMode: Text.WordWrap
      }

      Repeater {
        model: modelData.deployments || []
        delegate: deploymentDelegate
      }
    }
  }

  Component {
    id: deploymentDelegate
    RowLayout {
      width: root.serverColumnWidth
      spacing: Style.space(8)

      Text {
        Layout.alignment: Qt.AlignTop
        text: Model.statusGlyph(modelData.status)
        textFormat: Text.PlainText
        color: Model.isActive(modelData.status) ? root.active
          : (modelData.status === "failed" ? root.urgent
            : Qt.darker(root.foreground, 1.4))
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
      }

      // Wraps inside the column instead of eliding to a stub, so long
      // commit messages stay readable and never spill into the next
      // server column.
      Text {
        Layout.fillWidth: true
        Layout.alignment: Qt.AlignTop
        text: Model.deploymentText(modelData)
        textFormat: Text.PlainText
        color: Model.isActive(modelData.status) ? root.foreground
          : Qt.darker(root.foreground, 1.4)
        font.family: root.fontFamily
        font.pixelSize: Style.font.body
        wrapMode: Text.WordWrap
      }

      Text {
        Layout.alignment: Qt.AlignTop
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
