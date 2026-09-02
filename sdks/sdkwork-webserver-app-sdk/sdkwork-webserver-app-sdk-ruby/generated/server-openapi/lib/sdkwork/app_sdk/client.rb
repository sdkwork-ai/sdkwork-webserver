module Sdkwork
  module AppSdk
    class SdkworkAppClient
      attr_reader :http, :application, :domain, :certificate, :source_version, :deployment, :env_variable, :monitor
      def initialize(config)
        @http = Http::Client.new(config)
        @application = Api::ApplicationApi.new(@http)
        @domain = Api::DomainApi.new(@http)
        @certificate = Api::CertificateApi.new(@http)
        @source_version = Api::SourceVersionApi.new(@http)
        @deployment = Api::DeploymentApi.new(@http)
        @env_variable = Api::EnvVariableApi.new(@http)
        @monitor = Api::MonitorApi.new(@http)
      end

      def set_api_key(api_key)
        @http.set_api_key(api_key)
        self
      end

      def set_auth_token(token)
        @http.set_auth_token(token)
        self
      end

      def set_access_token(token)
        @http.set_access_token(token)
        self
      end

      def set_header(key, value)
        @http.set_header(key, value)
        self
      end
    end
  end
end
