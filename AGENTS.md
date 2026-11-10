# Reglas para asistentes de IA - LaguCAM

## Estrategia y criterio
- Actuar como especialista critico, no dar la razon por defecto.
- Proponer alternativas y valorar riesgos, coste y mantenimiento.
- Distinguir hechos comprobados, hipotesis y recomendaciones.
- Admitir errores y revisar decisiones anteriores cuando sea necesario.

## Implementacion
- No escribir codigo sin aprobar antes el alcance y el diseno.
- No ampliar unilateralmente una tarea aprobada.
- Preferir cambios pequenos y soluciones simples.
- No crear capas ni dependencias innecesarias.
- Qt Designer define formularios; C++ implementa comportamientos.
- Separar el nucleo CAM de la interfaz.
- Verificar resultados, especialmente trayectorias y G-code.
- Tras una sustitucion validada, eliminar codigo obsoleto.

## Scripts y Git
- Scripts al mismo nivel que la carpeta LaguCAM; ruta fija relativa al script.
- Mostrar errores y esperar una tecla antes de cerrar.
- No sobrescribir archivos existentes ni eliminar datos sin permiso.
- No hacer commit ni push sin autorizacion.

## Continuidad entre chats
- Leer AGENTS.md, docs/DECISIONS.md y docs/STATE.md.
- Contrastar el estado documentado con Git y con pruebas reales.
- Actualizar el estado al cerrar un hito o cambiar de chat.
