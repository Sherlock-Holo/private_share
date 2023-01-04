// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'list_file_response.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

ListFileResponse _$ListFileResponseFromJson(Map<String, dynamic> json) =>
    ListFileResponse(
      (json['files'] as List<dynamic>)
          .map((e) => ListFile.fromJson(e as Map<String, dynamic>))
          .toList(),
    );

Map<String, dynamic> _$ListFileResponseToJson(ListFileResponse instance) =>
    <String, dynamic>{
      'files': instance.files,
    };

ListFile _$ListFileFromJson(Map<String, dynamic> json) => ListFile(
      json['filename'] as String,
      json['hash'] as String,
      json['downloaded'] as bool,
      (json['peers'] as List<dynamic>).map((e) => e as String).toList(),
      json['size'] as String,
    );

Map<String, dynamic> _$ListFileToJson(ListFile instance) => <String, dynamic>{
      'filename': instance.filename,
      'hash': instance.hash,
      'downloaded': instance.downloaded,
      'peers': instance.peers,
      'size': instance.size,
    };
