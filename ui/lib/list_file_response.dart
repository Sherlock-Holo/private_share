import 'package:json_annotation/json_annotation.dart';

part 'list_file_response.g.dart';

@JsonSerializable()
class ListFileResponse {
  final List<ListFile> files;

  ListFileResponse(this.files);

  factory ListFileResponse.fromJson(Map<String, dynamic> json) =>
      _$ListFileResponseFromJson(json);

  Map<String, dynamic> toJson() => _$ListFileResponseToJson(this);
}

@JsonSerializable()
class ListFile {
  final String filename;
  final String hash;
  final bool downloaded;
  final List<String> peers;
  final String size;

  ListFile(this.filename, this.hash, this.downloaded, this.peers, this.size);

  factory ListFile.fromJson(Map<String, dynamic> json) =>
      _$ListFileFromJson(json);

  Map<String, dynamic> toJson() => _$ListFileToJson(this);
}
