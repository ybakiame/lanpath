package com.ybakiame.lanpath

import android.content.Context
import android.content.ContentValues
import android.os.Environment
import android.provider.MediaStore
import java.io.File
import java.io.OutputStream
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.zip.ZipEntry
import java.util.zip.ZipOutputStream

/** Downloads exports independent of the eventual mobile UI runtime. */
class MobileFiles(private val context: Context) {
  fun exportLogs(): String {
    val files = collectLogFiles()
    check(files.isNotEmpty()) { "暂无可导出的日志" }
    return exportViaMediaStore(files, "Lanpath-logs-${timestampFileName()}.zip")
  }

  fun exportSubscription(filename: String, content: String): String {
    require(Regex("Lanpath-subscription-[a-f0-9-]+\\.(yaml|json|txt)").matches(filename)) { "无效的导出文件名" }
    return writeDownload(filename, "text/plain", content.toByteArray(Charsets.UTF_8))
  }

  fun exportBackup(filename: String, bytes: ByteArray): String {
    require(Regex("Lanpath-backup-[a-f0-9-]+\\.zip\\.age").matches(filename)) { "无效的备份文件名" }
    require(bytes.size <= 16 * 1024 * 1024 && bytes.take(21).toByteArray()
      .toString(Charsets.US_ASCII).startsWith("age-encryption.org/v1")) { "备份格式无效" }
    return writeDownload(filename, "application/octet-stream", bytes)
  }

  private fun writeDownload(filename: String, mime: String, bytes: ByteArray): String {
    val resolver = context.contentResolver
    val values = ContentValues().apply {
      put(MediaStore.MediaColumns.DISPLAY_NAME, filename)
      put(MediaStore.MediaColumns.MIME_TYPE, mime)
      put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Lanpath/")
      put(MediaStore.MediaColumns.IS_PENDING, 1)
    }
    val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
      ?: error("无法创建下载文件")
    try {
      resolver.openOutputStream(uri)?.use { it.write(bytes) } ?: error("无法写入下载文件")
      check(resolver.update(uri, ContentValues().apply {
        put(MediaStore.MediaColumns.IS_PENDING, 0)
      }, null, null) == 1) { "无法提交下载文件" }
    } catch (e: Exception) {
      resolver.delete(uri, null, null)
      throw e
    }
    return "Download/Lanpath/$filename"
  }

  private fun collectLogFiles(): Array<File> {
    val logsDir = File(context.filesDir, "logs")
    return logsDir.listFiles { f ->
      f.isFile &&
        (f.name.endsWith(".log") ||
          f.name.startsWith("app.log") ||
          f.name == "last_start_config.json")
    } ?: emptyArray()
  }

  private fun timestampFileName(): String =
    SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())

  private fun exportViaMediaStore(logFiles: Array<File>, fileName: String): String {
    val values =
      ContentValues().apply {
        put(MediaStore.MediaColumns.DISPLAY_NAME, fileName)
        put(MediaStore.MediaColumns.MIME_TYPE, "application/zip")
        put(MediaStore.MediaColumns.RELATIVE_PATH, Environment.DIRECTORY_DOWNLOADS + "/Lanpath/")
      }
    val resolver = context.contentResolver
    val uri =
      resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
        ?: throw IllegalStateException("MediaStore 插入失败")
    try {
      resolver.openOutputStream(uri)?.use { writeZip(logFiles, it) }
        ?: throw IllegalStateException("无法打开 MediaStore 输出流")
    } catch (e: Exception) {
      resolver.delete(uri, null, null)
      throw e
    }
    return "Download/Lanpath/$fileName"
  }

  private fun writeZip(logFiles: Array<File>, output: OutputStream) {
    ZipOutputStream(output.buffered()).use { zip ->
      for (file in logFiles) {
        zip.putNextEntry(ZipEntry(file.name))
        file.inputStream().use { it.copyTo(zip) }
        zip.closeEntry()
      }
    }
  }

}
