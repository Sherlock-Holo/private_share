import 'dart:convert';

import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:ui/file_detail.dart';
import 'package:ui/list_file_response.dart';
import 'package:ui/peer_info.dart';

void main() {
  runApp(const MainApp());
}

class MainApp extends StatelessWidget {
  const MainApp({super.key});

  // This widget is the root of your application.
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'private share',
      theme: ThemeData(
        // This is the theme of your application.
        //
        // Try running your application with "flutter run". You'll see the
        // application has a blue toolbar. Then, without quitting the app, try
        // changing the primarySwatch below to Colors.green and then invoke
        // "hot reload" (press "r" in the console where you ran "flutter run",
        // or simply save your changes to "hot reload" in a Flutter IDE).
        // Notice that the counter didn't reset back to zero; the application
        // is not restarted.
        primarySwatch: Colors.blue,
      ),
      home: const HomePage(title: 'private share'),
    );
  }
}

class HomePage extends StatefulWidget {
  const HomePage({super.key, required this.title});

  // This widget is the home page of your application. It is stateful, meaning
  // that it has a State object (defined below) that contains fields that affect
  // how it looks.

  // This class is the configuration for the state. It holds the values (in this
  // case the title) provided by the parent (in this case the App widget) and
  // used by the build method of the State. Fields in a Widget subclass are
  // always marked "final".

  final String title;

  @override
  State<HomePage> createState() => _HomePageState();
}

class _HomePageState extends State<HomePage> {
  List<FileDetail> files = [];
  List<PeerInfo> peers = [];

  Future<FileList> _getFileList() async {
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

    return FileList(
        files: listResp.files.map((file) {
      return FileDetail(
          filename: file.filename,
          hash: file.hash,
          size: file.size,
          downloaded: file.downloaded);
    }).toList());
  }

  @override
  Widget build(BuildContext context) {
    // This method is rerun every time setState is called, for instance as done
    // by the _incrementCounter method above.
    //
    // The Flutter framework has been optimized to make rerunning build methods
    // fast, so that you can just rebuild anything that needs updating rather
    // than having to individually change instances of widgets.
    return Scaffold(
      appBar: AppBar(
        // Here we take the value from the MyHomePage object that was created by
        // the App.build method, and use it to set our appbar title.
        title: Text(widget.title),
      ),
      drawer: Drawer(
        child: PeerList(peers: peers),
      ),
      body: Center(
        // Center is a layout widget. It takes a single child and positions it
        // in the middle of the parent.
        child: FutureBuilder<FileList>(
          future: _getFileList(),
          builder: (BuildContext context, AsyncSnapshot<FileList> snapshot) {
            if (snapshot.hasData) {
              return snapshot.data!;
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
        ),
      ),
    );
  }
}
