import 'package:flutter/material.dart';

class FileList extends StatelessWidget {
  final List<FileDetail> files;

  const FileList({super.key, required this.files});

  @override
  Widget build(BuildContext context) {
    return ListView.builder(
      itemCount: files.length,
      itemBuilder: (context, index) {
        var file = files[index];

        return ListTile(
          title: Text(file.filename),
          trailing: Icon(
              file.downloaded ? Icons.download_done : Icons.download_outlined),
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
