# Demandas de compatibilidade e experiência de containers

Este documento reúne demandas para aproximar o projeto da experiência oferecida pelo Docker Desktop e pelo ecossistema construído ao redor do Docker Engine.

Ficam fora deste escopo críticas relacionadas à exclusividade de hardware ou sistemas operacionais da Apple.

## 1. Suporte a Docker Compose

### Demanda

Permitir a definição e a execução de aplicações com múltiplos containers por meio do formato Compose.

### Requisitos esperados

- Interpretar arquivos `compose.yaml` e `docker-compose.yml`.
- Implementar os principais comandos do fluxo Compose, incluindo `up`, `down`, `build`, `pull`, `start`, `stop`, `restart`, `ps`, `logs`, `exec` e `config`.
- Respeitar dependências entre serviços e sua ordem de inicialização e encerramento.
- Suportar redes, volumes, variáveis de ambiente, health checks, portas e secrets definidos no arquivo.
- Identificar e gerenciar recursos por projeto e serviço.
- Fornecer mensagens claras quando uma opção do padrão Compose ainda não for suportada.

## 2. Compatibilidade com a API do Docker Engine

### Demanda

Oferecer uma camada compatível com a API do Docker Engine e com os mecanismos de conexão esperados pelas ferramentas existentes.

### Requisitos esperados

- Disponibilizar um endpoint compatível com o Docker socket ou uma alternativa configurável equivalente.
- Implementar as operações essenciais de containers, imagens, redes, volumes, builds, logs e execução de comandos.
- Documentar explicitamente o nível de compatibilidade por versão da API.
- Retornar erros previsíveis para endpoints ou opções ainda não implementados.

## 3. Integração com ferramentas do ecossistema Docker

### Demanda

Permitir que ferramentas que dependem do Docker Engine funcionem sem adaptações específicas ou com configuração mínima.

### Requisitos esperados

- Validar compatibilidade com Testcontainers.
- Integrar com Dev Containers.
- Permitir uso por IDEs e extensões que detectam um Docker Engine.
- Suportar ferramentas de desenvolvimento que acessam o Docker socket.
- Manter uma matriz pública de compatibilidade e testes automatizados para as integrações principais.

## 4. Experiência integrada de gerenciamento

### Demanda

Fornecer recursos de administração e diagnóstico comparáveis aos fluxos mais usados no Docker Desktop.

### Requisitos esperados

- Exibir containers, imagens, redes, volumes e consumo de recursos.
- Facilitar a consulta e o acompanhamento de logs.
- Permitir iniciar, parar, reiniciar, inspecionar e remover recursos.
- Expor diagnósticos de rede, armazenamento e runtime.
- Integrar o gerenciamento de credenciais de registries.
- Disponibilizar pontos de extensão para ferramentas complementares.

Uma interface gráfica pode atender parte desta demanda, mas não deve ser obrigatória para automação ou uso da CLI.

## 5. Compatibilidade e confiabilidade de builds

### Demanda

Executar Dockerfiles e fluxos de build OCI com comportamento previsível e compatível com as práticas do ecossistema Docker.

### Requisitos esperados

- Garantir acesso de rede confiável durante o build.
- Suportar builds multi-stage.
- Oferecer cache de camadas consistente e controlável.
- Permitir configuração de rede, DNS, proxies, certificados e registries privados.
- Documentar diferenças de comportamento em relação ao BuildKit.
- Criar uma suíte de compatibilidade com Dockerfiles de projetos reais.

## 6. Rede e DNS confiáveis

### Demanda

Tornar previsível a comunicação entre containers, entre host e container e entre containers e serviços externos.

### Requisitos esperados

- Resolver nomes de serviços e domínios externos de forma consistente.
- Suportar redes isoladas e múltiplas redes por aplicação.
- Implementar publicação e encaminhamento de portas de maneira estável.
- Permitir comunicação eficiente entre containers.
- Oferecer configuração de DNS, proxy e regras de acesso.
- Incluir ferramentas de diagnóstico e mensagens de erro acionáveis.

## 7. Desempenho de bind mounts

### Demanda

Reduzir a penalidade de desempenho ao compartilhar diretórios do host com containers, especialmente em projetos com muitos arquivos pequenos.

### Requisitos esperados

- Otimizar operações de leitura, escrita e metadados em bind mounts.
- Criar benchmarks reproduzíveis para cargas como `node_modules`, instalação de dependências e compilação incremental.
- Oferecer estratégias documentadas de cache e sincronização.
- Recomendar automaticamente volumes nativos ou `tmpfs` quando forem mais adequados.
- Preservar consistência e observação de mudanças entre host e container.

## 8. Desempenho e densidade do modelo de uma VM por container

### Demanda

Reduzir o custo de inicialização, memória e comunicação introduzido pelo isolamento de uma máquina virtual por container.

### Requisitos esperados

- Reduzir o tempo de criação e inicialização de containers.
- Melhorar throughput e latência na comunicação entre containers.
- Permitir executar vários containers simultaneamente sem degradação desproporcional.
- Medir continuamente startup, uso de memória, CPU, disco e rede contra runtimes de referência.
- Avaliar um mecanismo opcional de agrupamento de containers relacionados sem eliminar o modelo isolado atual.

## 9. Devolução de memória ao host

### Demanda

Garantir que memória liberada por processos dentro dos containers possa ser recuperada pelo host sem exigir a reinicialização do container.

### Requisitos esperados

- Implementar ou aprimorar o mecanismo de memory ballooning disponível.
- Tornar visíveis memória configurada, memória em uso e memória efetivamente retida pela VM.
- Evitar crescimento permanente do consumo após picos de carga.
- Oferecer uma operação segura de recuperação de memória quando a devolução automática não for possível.

## 10. Aceleração por GPU e workloads especializados

### Demanda

Viabilizar workloads que dependem de aceleração gráfica ou computacional dentro dos containers.

### Requisitos esperados

- Definir uma arquitetura segura para expor recursos de GPU aos containers.
- Oferecer detecção de capacidades e erros claros quando a aceleração não estiver disponível.
- Documentar APIs, drivers e limitações suportadas.
- Cobrir casos de uso de computação, inferência, processamento de mídia e aplicações gráficas.

## Priorização sugerida

1. Compatibilidade com a API do Docker Engine.
2. Suporte a Docker Compose.
3. Integrações com Testcontainers, Dev Containers e IDEs.
4. Confiabilidade de builds, rede e DNS.
5. Desempenho de armazenamento, rede e inicialização.
6. Gerenciamento integrado e diagnóstico.
7. Recuperação de memória.
8. Aceleração por GPU e workloads especializados.

## Critério geral de evolução

Cada demanda deve possuir testes automatizados, documentação de limitações e uma matriz de compatibilidade. Funcionalidades parcialmente implementadas devem falhar de forma explícita, sem produzir comportamento silenciosamente diferente daquele esperado por ferramentas do ecossistema Docker.
