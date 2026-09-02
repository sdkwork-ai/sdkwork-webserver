module Sdkwork
  module BackendSdk
    module Models
      class ServerEntry
              attr_accessor :name, :kind, :path, :size, :project_type, :is_project_root

              def initialize(attributes = {})
                attributes = (attributes || {}).transform_keys(&:to_s)
                @name = attributes['name']
                @kind = attributes['kind']
                @path = attributes['path']
                @size = attributes['size']
                @project_type = attributes['projectType']
                @is_project_root = attributes['isProjectRoot']
              end

              def self.from_hash(data)
                return nil if data.nil?

                new(data)
              end

              def to_hash
                {
                  'name' => @name,
                  'kind' => @kind,
                  'path' => @path,
                  'size' => @size,
                  'projectType' => @project_type,
                  'isProjectRoot' => @is_project_root,
                }
              end
            end
    end
  end
end
