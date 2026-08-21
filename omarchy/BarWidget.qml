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
  property var currentActionArgs: []

  // Latest raw line from the elected owner's watch stream. Peers read this
  // through applySnapshot() to stay in sync across monitors.
  property string lastSnapshotLine: ""

  readonly property var panelItem: panelLoader.item
  readonly property bool opened: panelItem ? panelItem.opened === true : false

  // The last parsed snapshot from the watch stream (null until the first
  // line arrives). Assigned as a whole so the panel sees one consistent view.
  property var status: null

  readonly property bool hideWhenIdle: setting("hideWhenIdle", false) === true
  readonly property bool hasError: status !== null && status.state === "error"
  readonly property bool hasActivity: status !== null && status.state === "ok"
    && Model.totals(status).running > 0

  // WidgetButton renders with AutoText, which only enters rich-text mode on
  // a raw "<". Entity-escaped text has no "<" and would draw the entities
  // verbatim, so the bar label and tooltip strip markup characters instead.
  readonly property string labelText: Model.stripMarkup(Model.labelText(status))
  readonly property string tooltipText: Model.stripMarkup(Model.tooltipText(status))
  readonly property color urgent: bar ? bar.urgent : Color.urgent

  // Every live per-monitor instance of this widget, straight from the host
  // registry. Read as a function (not a binding) so re-elections after a
  // monitor goes away see the current list.
  function livePeers() {
    return root.bar && typeof root.bar.moduleWidgets === "function"
      ? root.bar.moduleWidgets(root.moduleName) : [root]
  }

  // The instance elected to run the single `coolify-qs watch` process: the
  // first in the registry. All others only relay state via broadcast().
  function watchOwner() {
    var items = root.livePeers()
    return (Array.isArray(items) && items.length > 0) ? items[0] : root
  }

  function ownsWatch() {
    return root.watchOwner() === root
  }

  // Start the watch process if this instance is the elected owner. The host
  // injects `bar` after the widget is created, so Component.onCompleted runs
  // before the registry is reachable; ownership is (re)decided here and on
  // every bar change and election tick instead.
  function maybeStartWatch() {
    if (!root.bar) return
    if (!root.ownsWatch()) return
    if (watchProc.running || watchRestartTimer.running) return
    watchProc.running = true
  }

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

  // Re-apply the elected owner's latest line. Late-joining peers call this
  // through broadcast() so they sync immediately instead of waiting for the
  // next snapshot change.
  function applySnapshot() {
    var owner = root.watchOwner()
    root.applyLine(owner && typeof owner.lastSnapshotLine === "string"
      ? owner.lastSnapshotLine : "")
  }

  function clearStatus() {
    root.status = null
  }

  function runAction(args) {
    if (!args || !args.length) return
    if (actionProc.running) {
      // A click while an action is already running: keep the latest request
      // and run it once the current process exits instead of dropping it.
      root.pendingActionArgs = args
      return
    }
    root.currentActionArgs = args
    root.pendingActionArgs = []
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

  onBarChanged: {
    injectPanel()
    root.maybeStartWatch()
    // A late-joining peer (a monitor that came up after the owner) has no
    // snapshot of its own; pull the owner's latest line across now.
    root.broadcast("applySnapshot")
  }
  onSettingsChanged: injectPanel()

  Component.onCompleted: {
    root.maybeStartWatch()
    root.broadcast("applySnapshot")
  }

  // Re-check ownership periodically: when the elected owner's monitor goes
  // away, the next instance in the registry takes over the watch process.
  Timer {
    id: electionTimer
    interval: 5000
    repeat: true
    running: true
    onTriggered: root.maybeStartWatch()
  }

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
      onRead: function(line) {
        root.lastSnapshotLine = line
        root.applyLine(line)
        root.broadcast("applySnapshot")
      }
    }
    onStarted: {
      watchProc.startedOnce = true
      root.watchFailures = 0
    }
    onExited: {
      root.broadcast("clearStatus")
      watchRestartTimer.restart()
    }
    onRunningChanged: {
      if (watchProc.running) return
      var failedStart = !watchProc.startedOnce
      watchProc.startedOnce = false
      if (failedStart) {
        root.broadcast("clearStatus")
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
      if (failedStart && actionProc.retried) {
        // Both binaries failed to start: drop this request and let the
        // next click start fresh.
        actionProc.retried = false
        root.currentActionArgs = []
        root.pendingActionArgs = []
        root.actionFailures += 1
        if (root.actionFailures >= root.fallbackThreshold) {
          root.actionFailures = 0
          root.actionFallback = !root.actionFallback
        }
        return
      }
      if (failedStart) {
        // First failed start: retry once with the alternate binary instead
        // of respawning the same failing one.
        actionProc.retried = true
        root.actionFallback = !root.actionFallback
      } else {
        actionProc.retried = false
        root.currentActionArgs = []
      }
      // Prefer a click that arrived while the process was running; the
      // retry's own args only survive when nothing new came in.
      var args = root.pendingActionArgs.length
        ? root.pendingActionArgs : root.currentActionArgs
      if (!args || !args.length) return
      root.pendingActionArgs = []
      root.currentActionArgs = args
      actionProc.command = [root.actionBinary].concat(args)
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
