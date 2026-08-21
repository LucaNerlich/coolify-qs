import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import qs.Ui
import "Model.js" as Model

// Quattro bar entry point for Coolify deployments. All API traffic lives in
// the Rust backend (`coolify-qs watch`); this file owns the bar button, the
// panel routing, and the watch / action process lifecycle.
BarWidget {
  id: root
  moduleName: "luca.coolify"

  function decodeFileUrl(urlString) {
    var path = String(urlString).replace(/^file:\/\//, "")
    try {
      return decodeURIComponent(path)
    } catch (e) {
      return path
    }
  }
  readonly property string bundledBinary: root.decodeFileUrl(
    Qt.resolvedUrl("bin/coolify-qs").toString())
  readonly property int fallbackThreshold: 2
  property bool watchFallback: false
  property bool actionFallback: false
  property int watchFailures: 0
  property int actionFailures: 0
  readonly property string watchBinary: watchFallback ? "coolify-qs" : bundledBinary
  readonly property string actionBinary: actionFallback ? "coolify-qs" : bundledBinary
  property var pendingActionArgs: []

  readonly property var panelItem: panelLoader.item
  readonly property bool opened: panelItem ? panelItem.opened === true : false

  // The last parsed snapshot from the watch stream (null until the first
  // line arrives). Assigned as a whole so the panel sees one consistent view.
  property var status: null

  readonly property bool hideWhenIdle: setting("hideWhenIdle", false) === true
  readonly property bool hasError: status !== null && status.state === "error"
  readonly property bool hasActivity: status !== null && status.state === "ok"
    && Model.totals(status).running > 0

  // WidgetButton renders with AutoText, so the bar label and tooltip — both
  // of which can contain Coolify-controlled app/server names — are escaped
  // before they reach the component.
  readonly property string labelText: Model.escapeText(Model.labelText(status))
  readonly property string tooltipText: Model.escapeText(Model.tooltipText(status))
  readonly property color urgent: bar ? bar.urgent : Color.urgent

  function open() { if (panelItem) panelItem.open() }
  function close() { if (panelItem) panelItem.close() }
  function toggle() {
    if (!panelItem) return
    if (panelItem.opened === true) root.close()
    else root.open()
  }

  function openUrl(url) {
    var u = String(url || "").trim()
    if (u === "") return
    root.runAction(["open", "--url", u])
  }

  function applyLine(line) {
    var parsed = Model.parseLine(String(line || ""))
    if (parsed) root.status = parsed
  }

  function clearStatus() {
    root.status = null
  }

  function runAction(args) {
    if (actionProc.running) return
    if (!args || !args.length) return
    root.pendingActionArgs = args
    actionProc.retried = false
    actionProc.command = [root.actionBinary].concat(args)
    actionProc.running = true
  }

  function injectPanel() {
    var target = panelItem
    if (!target) return
    if ("bar" in target) target.bar = root.bar
    if ("settings" in target) target.settings = root.settings
    if ("anchorItem" in target) target.anchorItem = button
    if ("hostWidget" in target) target.hostWidget = root
  }

  visible: !hideWhenIdle || !Model.isIdle(status)
  implicitWidth: button.implicitWidth
  implicitHeight: button.implicitHeight

  onBarChanged: injectPanel()
  onSettingsChanged: injectPanel()

  Component.onCompleted: watchProc.running = true

  Loader {
    id: panelLoader
    active: true
    source: Qt.resolvedUrl("Panel.qml")
    visible: false
    onLoaded: {
      root.injectPanel()
      Qt.callLater(root.injectPanel)
    }
  }

  Process {
    id: watchProc
    command: [root.watchBinary, "watch"]
    property bool startedOnce: false
    stdout: SplitParser {
      onRead: function(line) { root.applyLine(line) }
    }
    onStarted: {
      watchProc.startedOnce = true
      root.watchFailures = 0
    }
    onExited: {
      root.clearStatus()
      watchRestartTimer.restart()
    }
    onRunningChanged: {
      if (watchProc.running) return
      var failedStart = !watchProc.startedOnce
      watchProc.startedOnce = false
      if (failedStart) {
        root.clearStatus()
        root.watchFailures += 1
        if (root.watchFailures >= root.fallbackThreshold) {
          root.watchFailures = 0
          root.watchFallback = !root.watchFallback
        }
      }
      watchRestartTimer.restart()
    }
  }

  Timer {
    id: watchRestartTimer
    interval: 5000
    repeat: false
    onTriggered: watchProc.running = true
  }

  Process {
    id: actionProc
    property bool startedOnce: false
    property bool retried: false
    onStarted: {
      actionProc.startedOnce = true
      root.actionFailures = 0
    }
    onRunningChanged: {
      if (actionProc.running) return
      var failedStart = !actionProc.startedOnce
      actionProc.startedOnce = false
      if (!failedStart || root.pendingActionArgs.length === 0) {
        root.pendingActionArgs = []
        return
      }
      if (actionProc.retried) {
        actionProc.retried = false
        root.pendingActionArgs = []
        root.actionFailures += 1
        if (root.actionFailures >= root.fallbackThreshold) {
          root.actionFailures = 0
          root.actionFallback = !root.actionFallback
        }
        return
      }
      actionProc.retried = true
      actionProc.command = [root.actionBinary].concat(root.pendingActionArgs)
      actionProc.running = true
    }
  }

  WidgetButton {
    id: button
    anchors.fill: parent
    bar: root.bar
    text: root.labelText
    foreground: root.hasError ? root.urgent : Color.bar.text
    activeColor: Color.bar.active
    active: root.hasActivity
    horizontalMargin: 8.5
    verticalPadding: 6
    tooltipText: root.tooltipText
    onPressed: function(buttonCode) {
      if (buttonCode === Qt.LeftButton || buttonCode === Qt.RightButton)
        root.toggle()
    }
  }
}
