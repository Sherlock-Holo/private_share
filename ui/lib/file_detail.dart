import 'package:flutter/material.dart';
import 'package:mime/mime.dart';

class FileList extends StatelessWidget {
  final List<FileDetail> files;

  const FileList({super.key, required this.files});

  @override
  Widget build(BuildContext context) {
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

  const FileDetail(
      {required this.filename,
      required this.hash,
      required this.size,
      required this.downloaded});
}
