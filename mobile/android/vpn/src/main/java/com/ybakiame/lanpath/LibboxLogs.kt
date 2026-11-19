package com.ybakiame.lanpath

import android.util.Log
import com.ybakiame.lanpath.core.libbox.CommandClient
import com.ybakiame.lanpath.core.libbox.CommandClientHandler
import com.ybakiame.lanpath.core.libbox.CommandClientOptions
import com.ybakiame.lanpath.core.libbox.ConnectionEvents
import com.ybakiame.lanpath.core.libbox.Libbox
import com.ybakiame.lanpath.core.libbox.LogIterator
import com.ybakiame.lanpath.core.libbox.OutboundGroupItemIterator
import com.ybakiame.lanpath.core.libbox.OutboundGroupIterator
import com.ybakiame.lanpath.core.libbox.StatusMessage
import com.ybakiame.lanpath.core.libbox.StringIterator
import java.io.File
import java.time.OffsetDateTime
import java.time.format.DateTimeFormatter

internal class LibboxLogs(private val context: android.content.Context) {
  companion object {
    private const val TAG = "LibboxLogs"
    const val BUFFER_SIZE = 200
    private const val LIBBOX_LOG_MAX_BYTES = 1024 * 1024
    private val LOG_TS_FORMATTER = DateTimeFormatter.ofPattern("yyyy-MM-dd'T'HH:mm:ss.SSSXXX")
    // Shared across service instances so a restart disconnects the previous log client.
    private val libboxLogBuffer = ArrayDeque<String>(BUFFER_SIZE)
    private var libboxLogFileResetPending = false
    @Volatile private var commandClient: CommandClient? = null
  }
  private val filesDir get() = context.filesDir
  fun start() {
    stop()
    val options =
      CommandClientOptions().apply {
        addCommand(Libbox.CommandLog)
        statusInterval = 1_000_000_000L
      }
    val client =
      try {
        CommandClient(logClientHandler, options)
      } catch (e: Exception) {
        Log.e(TAG, "failed to create log command client", e)
        return
      }
    commandClient = client
    Thread {
      try {
        client.connect()
      } catch (e: Exception) {
        Log.e(TAG, "log command client connect failed", e)
      }
    }.start()
  }

  fun stop() {
    val client = commandClient
    commandClient = null
    if (client != null) {
      Thread {
        runCatching { client.disconnect() }
      }.start()
    }
  }

  private val logClientHandler =
    object : CommandClientHandler {
      override fun connected() {
        Log.i(TAG, "libbox log channel connected")
      }

      override fun disconnected(message: String?) {
        Log.i(TAG, "libbox log channel disconnected: $message")
      }

      override fun setDefaultLogLevel(level: Int) {}

      override fun clearLogs() {
        clearLibboxLogBuffer()
      }

      override fun writeLogs(messageList: LogIterator?) {
        if (messageList == null) {
          return
        }
        val lines = ArrayList<String>()
        while (messageList.hasNext()) {
          val entry = messageList.next() ?: continue
          val message = entry.message ?: ""
          Log.i(TAG, message)
          lines.add("[${timestampNow()}] $message")
        }
        if (lines.isNotEmpty()) {
          appendLibboxLogs(lines)
          appendToLibboxLogFile(lines)
        }
      }

      override fun writeStatus(message: StatusMessage?) {}

      override fun writeGroups(message: OutboundGroupIterator?) {}

      override fun writeOutbounds(message: OutboundGroupItemIterator?) {}

      override fun initializeClashMode(modeList: StringIterator?, currentMode: String?) {}

      override fun updateClashMode(newMode: String?) {}

      override fun writeConnectionEvents(events: ConnectionEvents?) {}
    }

  private fun appendLibboxLogs(lines: List<String>) {
    synchronized(libboxLogBuffer) {
      for (line in lines) {
        libboxLogBuffer.addLast(line)
        while (libboxLogBuffer.size > BUFFER_SIZE) {
          libboxLogBuffer.removeFirst()
        }
      }
    }
  }

  private fun clearLibboxLogBuffer() {
    synchronized(libboxLogBuffer) {
      libboxLogBuffer.clear()
      libboxLogFileResetPending = true
    }
  }

  private fun libboxLogTail(lines: Int): String {
    synchronized(libboxLogBuffer) {
      return libboxLogBuffer.takeLast(lines).joinToString("\n")
    }
  }

  private fun timestampNow(): String = LOG_TS_FORMATTER.format(OffsetDateTime.now())

  private fun appendToLibboxLogFile(lines: List<String>) {
    try {
      val logDir = File(filesDir, "logs")
      if (!logDir.isDirectory && !logDir.mkdirs()) {
        return
      }
      val logFile = File(logDir, "libbox.log")
      val reset =
        synchronized(libboxLogBuffer) {
          val pending = libboxLogFileResetPending
          libboxLogFileResetPending = false
          pending
        }
      if (reset) {
        logFile.writeText("")
      } else if (logFile.exists() && logFile.length() > LIBBOX_LOG_MAX_BYTES) {
        logFile.delete()
        logFile.writeText(libboxLogTail(BUFFER_SIZE))
      }
      logFile.appendText(lines.joinToString("\n", postfix = "\n"))
    } catch (_: Exception) {}
  }

  fun withTail(message: String): String {
    val tail = libboxLogTail(15)
    return if (tail.isEmpty()) message else "$message\n--- libbox 日志 ---\n$tail"
  }
}
