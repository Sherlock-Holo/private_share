// GENERATED CODE - DO NOT MODIFY BY HAND

part of 'list_tv_response.dart';

// **************************************************************************
// JsonSerializableGenerator
// **************************************************************************

ListTVResponse _$ListTVResponseFromJson(Map<String, dynamic> json) =>
    ListTVResponse(
      json['friend_name'] as String,
      json['encoded_url'] as String,
    );

Map<String, dynamic> _$ListTVResponseToJson(ListTVResponse instance) =>
    <String, dynamic>{
      'friend_name': instance.friendName,
      'encoded_url': instance.encodedUrl,
    };
