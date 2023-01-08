import 'dart:convert';

import 'package:file_picker/file_picker.dart';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:http_parser/http_parser.dart';
import 'package:mime/mime.dart';
import 'package:ui/list_file_response.dart';

class FileList extends StatefulWidget {
  const FileList({super.key});

  @override
  State<StatefulWidget> createState() => _FileListState();
}

class _FileListState extends State<FileList> {
  Future<int> _uploadFile() async {
    final result = await FilePicker.platform
        .pickFiles(withData: false, withReadStream: true);
    if (result == null) {
      return 0;
    }

    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, "/api/upload_file");
    } else {
      url = Uri.https(Uri.base.authority, "/api/upload_file");
    }

    final file = result.files.single;
    final mimeType = lookupMimeType(file.name);
    final contentType = mimeType != null ? MediaType.parse(mimeType) : null;
    final multipartFile = http.MultipartFile(
        file.name, file.readStream!, file.size,
        filename: file.name, contentType: contentType);

    final request = http.MultipartRequest("POST", url)
      ..files.add(multipartFile);

    return (await request.send()).statusCode;
  }

  @override
  Widget build(BuildContext context) {
    return Expanded(
      flex: 2,
      child: Column(
        children: [
          ListTile(
            title: const Text(
              "File List",
              style: TextStyle(fontWeight: FontWeight.bold, fontSize: 30),
            ),
            trailing: Row(
              mainAxisSize: MainAxisSize.min,
              children: [
                IconButton(
                    onPressed: () {
                      _uploadFile().then((value) {
                        if (value == 0) {
                          return;
                        }

                        if (value != 200) {
                          ScaffoldMessenger.of(context).showSnackBar(SnackBar(
                              content: Text(
                            "Upload failed: $value",
                            style: const TextStyle(color: Colors.red),
                          )));
                        } else {
                          ScaffoldMessenger.of(context).showSnackBar(
                              const SnackBar(content: Text("Upload done")));
                        }

                        setState(() {});
                      });
                    },
                    icon: const Icon(Icons.add)),
                IconButton(
                    onPressed: () {
                      setState(() {});
                    },
                    icon: const Icon(Icons.refresh))
              ],
            ),
          ),
          Expanded(child: _createFileListWidget())
        ],
      ),
    );
  }

  FutureBuilder<List<FileDetail>> _createFileListWidget() {
    return FutureBuilder<List<FileDetail>>(
      future: _getFileList(),
      builder:
          (BuildContext context, AsyncSnapshot<List<FileDetail>> snapshot) {
        if (snapshot.hasData) {
          return _buildList(snapshot.data!);
        }

        if (snapshot.hasError) {
          return Text(
            "${snapshot.error!}",
            style: const TextStyle(color: Colors.red, fontSize: 20),
          );
        }

        return const SizedBox(
          width: 60,
          height: 60,
          child: CircularProgressIndicator(),
        );
      },
    );
  }

  Future<List<FileDetail>> _getFileList() async {
    Uri url;
    if (Uri.base.scheme == "http") {
      url = Uri.http(Uri.base.authority, "/api/list_files");
    } else {
      url = Uri.https(Uri.base.authority, "/api/list_files");
    }

    final resp = await http.get(url);
    if (resp.statusCode != 200) {
      throw Exception("status code ${resp.statusCode} != 200");
    }

    final respBody = utf8.decode(resp.bodyBytes);
    final json = jsonDecode(respBody) as Map<String, dynamic>;
    final listResp = ListFileResponse.fromJson(json);

    return listResp.files.map((file) {
      return FileDetail(
          filename: file.filename,
          hash: file.hash,
          size: file.size,
          downloaded: file.downloaded);
    }).toList();
  }

  Widget _buildList(List<FileDetail> files) {
    return ListView.builder(
      itemCount: files.length,
      itemBuilder: (context, index) {
        var file = files[index];

        Icon icon;
        final mimeType = lookupMimeType(file.filename)?.split("/");
        if (mimeType == null || mimeType.length != 2) {
          icon = const Icon(Icons.insert_drive_file_outlined);
        } else {
          final fileType = mimeType.first;
          switch (fileType) {
            case "audio":
              {
                icon = const Icon(Icons.audio_file_rounded);
              }
              break;
            case "image":
              {
                icon = const Icon(Icons.image_rounded);
              }
              break;
            case "video":
              {
                icon = const Icon(Icons.video_file_rounded);
              }
              break;
            default:
              {
                icon = const Icon(Icons.insert_drive_file_outlined);
              }
          }
        }

        return Card(
          child: ExpansionTile(
            leading: icon,
            title: SelectableText(file.filename),
            trailing: Icon(file.downloaded
                ? Icons.download_done
                : Icons.download_outlined),
            children: [
              const Divider(
                thickness: 1.0,
                height: 1.0,
              ),
              ListTile(leading: const Text("Size:"), title: Text(file.size)),
              ListTile(
                  leading: const Text("Hash:"),
                  title: SelectableText(file.hash))
            ],
          ),
        );
      },
    );
  }
}

class FileDetail {
  final String filename;
  final String hash;
  final String size;
  final bool downloaded;

  const FileDetail({required this.filename,
    required this.hash,
    required this.size,
    required this.downloaded});
}
