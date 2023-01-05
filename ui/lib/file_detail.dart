import 'package:flutter/material.dart';

class FileList extends StatelessWidget {
  final List<FileDetail> files;

  const FileList({super.key, required this.files});

  @override
  Widget build(BuildContext context) {
    return ListView.separated(
      itemCount: files.length,
      itemBuilder: (context, index) {
        var file = files[index];

        return Card(
          child: ListTile(
            leading: const Icon(Icons.insert_drive_file_outlined),
            title: SelectableText(file.filename),
            trailing: Icon(file.downloaded
                ? Icons.download_done
                : Icons.download_outlined),
            subtitle: Row(
              children: [
                const Text(
                  "Hash: ",
                  style: TextStyle(fontWeight: FontWeight.bold),
                ),
                Flexible(child: SelectableText(file.hash))
              ],
            ),
          ),
        );
      },
      separatorBuilder: (BuildContext context, int index) => const Divider(),
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
