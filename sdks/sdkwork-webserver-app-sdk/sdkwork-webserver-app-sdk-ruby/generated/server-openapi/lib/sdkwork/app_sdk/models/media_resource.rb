module Sdkwork
  module AppSdk
    module Models
      class MediaResource
              attr_accessor :id, :kind, :source, :url, :public_url, :uri, :object_blob_id, :file_name, :mime_type, :size_bytes, :checksum, :width, :height, :duration_seconds, :alt_text, :title, :metadata

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @id = attributes['id']
                @kind = attributes['kind']
                @source = attributes['source']
                @url = attributes['url']
                @public_url = attributes['publicUrl']
                @uri = attributes['uri']
                @object_blob_id = attributes['objectBlobId']
                @file_name = attributes['fileName']
                @mime_type = attributes['mimeType']
                @size_bytes = attributes['sizeBytes']
                @checksum = attributes['checksum'].is_a?(Hash) ? MediaChecksum.from_hash(attributes['checksum']) : nil
                @width = attributes['width']
                @height = attributes['height']
                @duration_seconds = attributes['durationSeconds']
                @alt_text = attributes['altText']
                @title = attributes['title']
                @metadata = attributes['metadata'].is_a?(Hash) ? attributes['metadata'] : {}
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'id' => @id,
                  'kind' => @kind,
                  'source' => @source,
                  'url' => @url,
                  'publicUrl' => @public_url,
                  'uri' => @uri,
                  'objectBlobId' => @object_blob_id,
                  'fileName' => @file_name,
                  'mimeType' => @mime_type,
                  'sizeBytes' => @size_bytes,
                  'checksum' => @checksum&.to_hash,
                  'width' => @width,
                  'height' => @height,
                  'durationSeconds' => @duration_seconds,
                  'altText' => @alt_text,
                  'title' => @title,
                  'metadata' => @metadata,
                }
              end
            end
    end
  end
end
